//! Session-owned selections and symmetric validation of pending and active request settings.
//! 会话自有选择及待生效与活动请求设置的对称校验。

use super::persistence::DirtyFile;
use super::{SessionDocument, ensure_api_request_field, validate_timestamp};
use crate::v1::config::{
    ActiveRequestConfig, KnowledgeLibrarySelectionConfig, KnowledgeLibrarySelectionMode,
    MetadataDocument, PendingRequestConfig, ProviderExtensionModeConfig, ProviderExtensionModeKind,
    ProviderExtensionTargetSetConfig, ProviderMediaUnderstandingMode,
    ProviderMediaUnderstandingRouteConfig, ProviderMediaUnderstandingSelection,
    ProviderToolCapabilitySelection, ProviderToolSelectionConfig, ProviderToolTargetConfig,
    RequestOptionsConfig, VmmSessionPolicyConfig,
};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ApiRequestSnapshot, ConversationEvent, EventBody, EventVisibility, RequestConfigChangedEvent,
};
use crate::v1::ids::{IdAllocator, Seq, TurnId};

/// Complete metadata-only session selection mutation committed as one domain revision.
/// 作为一个领域修订提交的完整纯 metadata 会话选择修改。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSelectionBundleInput {
    /// Optional session-owned output-style catalog identity.
    /// 可选的会话自有输出风格目录身份。
    pub output_style: Option<String>,
    /// Complete session-owned Provider tool selection.
    /// 完整的会话自有 Provider 工具选择。
    pub provider_tools: ProviderToolSelectionConfig,
    /// Complete session-owned knowledge-library selection.
    /// 完整的会话自有知识库选择。
    pub knowledge_libraries: KnowledgeLibrarySelectionConfig,
    /// Complete tri-state VMM session policy.
    /// 完整的 VMM 会话三态策略。
    pub vmm: VmmSessionPolicyConfig,
}

/// Normalizes one optional catalog or routing selection.
/// 规范化一个可选目录或路由选择值。
fn normalize_optional_selection(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized = value.trim();
        (!normalized.is_empty()).then(|| normalized.to_string())
    })
}

/// Validates one required session-selection component without normalization.
/// 校验一个必需会话选择组件，且不执行规范化。
fn validate_required_selection_component(
    field: &'static str,
    value: &str,
) -> Result<(), DomainError> {
    if value.is_empty() || value.trim() != value {
        return Err(DomainError::InvalidSelectionConfiguration { field });
    }
    Ok(())
}

/// Validates one exact Provider target identity.
/// 校验一个精确 Provider 目标身份。
fn validate_provider_tool_target_config(
    target: &ProviderToolTargetConfig,
) -> Result<(), DomainError> {
    validate_required_selection_component(
        "provider_tools.target.connection_id",
        &target.connection_id,
    )?;
    validate_required_selection_component("provider_tools.target.target_id", &target.target_id)
}

/// Validates one canonically ordered direct capability selection list.
/// 校验一份按规范排序的直接能力选择列表。
fn validate_provider_tool_capability_selections(
    selections: &[ProviderToolCapabilitySelection],
) -> Result<(), DomainError> {
    for selection in selections {
        validate_required_selection_component(
            "provider_tools.selections.capability",
            &selection.capability,
        )?;
        validate_provider_tool_target_config(&selection.target)?;
    }
    if selections
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.selections.order",
        });
    }
    Ok(())
}

/// Validates one canonically ordered media-mode selection list.
/// 校验一份按规范排序的媒体模式选择列表。
fn validate_provider_media_understanding_selections(
    selections: &[ProviderMediaUnderstandingSelection],
) -> Result<(), DomainError> {
    for selection in selections {
        validate_required_selection_component(
            "provider_tools.media_understanding_selections.capability",
            &selection.capability,
        )?;
        if selection.reasoning_effort.is_some()
            && selection.mode != ProviderMediaUnderstandingMode::ModelReplacement
        {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "provider_tools.media_understanding_selections.reasoning_effort_mode",
            });
        }
        match (&selection.mode, &selection.selection) {
            (
                ProviderMediaUnderstandingMode::ProviderTool,
                ProviderMediaUnderstandingRouteConfig::ProviderTool(target),
            ) => {
                validate_required_selection_component(
                    "provider_tools.media_understanding_selections.selection.connection_id",
                    &target.connection_id,
                )?;
                validate_required_selection_component(
                    "provider_tools.media_understanding_selections.selection.target_id",
                    &target.target_id,
                )?;
            }
            (
                ProviderMediaUnderstandingMode::ModelReplacement,
                ProviderMediaUnderstandingRouteConfig::ModelReplacement(target),
            ) => {
                validate_required_selection_component(
                    "provider_tools.media_understanding_selections.selection.provider_id",
                    &target.provider_id,
                )?;
                validate_required_selection_component(
                    "provider_tools.media_understanding_selections.selection.model_id",
                    &target.model_id,
                )?;
                validate_required_selection_component(
                    "provider_tools.media_understanding_selections.selection.connection_id",
                    &target.connection_id,
                )?;
            }
            _ => {
                return Err(DomainError::InvalidSelectionConfiguration {
                    field: "provider_tools.media_understanding_selections.selection_mode",
                });
            }
        }
    }
    if selections
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.media_understanding_selections.order",
        });
    }
    Ok(())
}

/// Validates one canonically ordered native-extension target-set list.
/// 校验一份按规范排序的原生扩展目标集合列表。
fn validate_provider_extension_target_sets(
    target_sets: &[ProviderExtensionTargetSetConfig],
) -> Result<(), DomainError> {
    for target_set in target_sets {
        validate_required_selection_component(
            "provider_tools.extension_target_sets.capability",
            &target_set.capability,
        )?;
        if target_set.targets.is_empty() {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "provider_tools.extension_target_sets.targets",
            });
        }
        for target in &target_set.targets {
            validate_provider_tool_target_config(target)?;
        }
        if target_set.targets.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(DomainError::InvalidSelectionConfiguration {
                field: "provider_tools.extension_target_sets.target_order",
            });
        }
    }
    if target_sets
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.extension_target_sets.order",
        });
    }
    Ok(())
}

/// Validates one canonically ordered list of non-default extension modes.
/// 校验一份按规范排序的非默认扩展模式列表。
fn validate_provider_extension_modes(
    modes: &[ProviderExtensionModeConfig],
) -> Result<(), DomainError> {
    for mode in modes {
        validate_required_selection_component(
            "provider_tools.extension_modes.capability",
            &mode.capability,
        )?;
        match (&mode.mode, &mode.single_target) {
            // The implicit default is never stored, so every entry carries information.
            // 隐式默认值从不保存，因此每个条目都携带信息。
            (ProviderExtensionModeKind::Single, None) => {
                return Err(DomainError::InvalidSelectionConfiguration {
                    field: "provider_tools.extension_modes.default_entry",
                });
            }
            (_, Some(target)) => validate_provider_tool_target_config(target)?,
            (ProviderExtensionModeKind::Multi, None) => {}
        }
    }
    if modes
        .windows(2)
        .any(|pair| pair[0].capability >= pair[1].capability)
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.extension_modes.order",
        });
    }
    Ok(())
}

/// Validates the complete session-owned Provider tool selection at commit and reload boundaries.
/// 在提交与重载边界校验当前会话持有的完整 Provider 工具选择。
pub(in crate::v1) fn validate_provider_tool_selection_config(
    config: &ProviderToolSelectionConfig,
) -> Result<(), DomainError> {
    if !config.initialized && !config.is_empty() {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.initialized",
        });
    }
    for capability in &config.enabled_capabilities {
        validate_required_selection_component("provider_tools.enabled_capabilities", capability)?;
    }
    if config
        .enabled_capabilities
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "provider_tools.enabled_capabilities.order",
        });
    }
    validate_provider_tool_capability_selections(&config.selections)?;
    validate_provider_media_understanding_selections(&config.media_understanding_selections)?;
    validate_provider_extension_target_sets(&config.extension_target_sets)?;
    validate_provider_extension_modes(&config.extension_modes)
}

/// Validates one complete knowledge-library selection at commit and reload boundaries.
/// 在提交与重载边界校验一份完整的知识库选择。
pub(super) fn validate_knowledge_library_selection_config(
    config: &KnowledgeLibrarySelectionConfig,
) -> Result<(), DomainError> {
    if config.mode == KnowledgeLibrarySelectionMode::Inherit && !config.library_ids.is_empty() {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.inherit_ids",
        });
    }
    for library_id in &config.library_ids {
        validate_required_selection_component("knowledge_libraries.library_ids", library_id)?;
    }
    if config.library_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(DomainError::InvalidSelectionConfiguration {
            field: "knowledge_libraries.library_ids.order",
        });
    }
    Ok(())
}

/// Validates one required request-routing selection without normalizing persisted facts.
/// 校验一个必需请求路由选择，且不规范化已持久化事实。
fn validate_required_request_selection(
    field: &'static str,
    value: &str,
) -> Result<(), DomainError> {
    if value.is_empty() || value.trim() != value {
        return Err(DomainError::InvalidRequestConfiguration { field });
    }
    Ok(())
}

/// Validates one optional request-routing selection without treating whitespace as absence.
/// 校验一个可选请求路由选择，且不把空白值当作缺省值。
fn validate_optional_request_selection(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), DomainError> {
    if let Some(value) = value {
        validate_required_request_selection(field, value)?;
    }
    Ok(())
}

/// Validates the provider-agnostic numeric request configuration.
/// 校验与 Provider 无关的数值请求配置。
fn validate_request_temperature(temperature: Option<f64>) -> Result<(), DomainError> {
    if temperature.is_some_and(|temperature| !temperature.is_finite()) {
        return Err(DomainError::InvalidRequestConfiguration {
            field: "temperature",
        });
    }
    Ok(())
}

/// Validates one exact physical API request snapshot at both commit and reload boundaries.
/// 在提交与重载边界同时校验一份精确物理 API 请求快照。
pub(super) fn validate_api_request_snapshot(
    request: &ApiRequestSnapshot,
) -> Result<(), DomainError> {
    validate_required_request_selection("provider", &request.provider)?;
    validate_optional_request_selection("profile", request.profile.as_deref())?;
    validate_required_request_selection("model", &request.model)?;
    validate_optional_request_selection("connection_id", request.connection_id.as_deref())?;
    validate_optional_request_selection("thinking_level", request.thinking_level.as_deref())?;
    validate_request_temperature(request.temperature)
}

/// Validates the authoritative active request configuration stored in metadata.
/// 校验保存在 metadata 中的权威当前请求配置。
pub(super) fn validate_active_request_config(
    config: &ActiveRequestConfig,
) -> Result<(), DomainError> {
    validate_optional_request_selection("active.provider", config.provider.as_deref())?;
    validate_optional_request_selection("active.connection_id", config.connection_id.as_deref())?;
    validate_optional_request_selection("active.model", config.model.as_deref())?;
    validate_optional_request_selection("active.thinking_level", config.thinking_level.as_deref())?;
    validate_request_temperature(config.temperature)
}

/// Validates one pending safe-boundary request configuration without guessing inheritance.
/// 校验一份待安全边界生效的请求配置，且不猜测继承语义。
pub(super) fn validate_pending_request_config(
    config: &PendingRequestConfig,
) -> Result<(), DomainError> {
    validate_optional_request_selection("pending.provider", config.provider.as_deref())?;
    validate_optional_request_selection("pending.connection_id", config.connection_id.as_deref())?;
    validate_optional_request_selection("pending.model", config.model.as_deref())?;
    validate_optional_request_selection(
        "pending.thinking_level",
        config.thinking_level.as_deref(),
    )?;
    validate_request_temperature(config.temperature)
}

/// Converts one exact API request snapshot into the session's active request configuration.
/// 把一份精确 API 请求快照转换为会话当前请求配置。
pub(super) fn active_request_from_snapshot(request: &ApiRequestSnapshot) -> ActiveRequestConfig {
    ActiveRequestConfig {
        provider: Some(request.provider.clone()),
        connection_id: request.connection_id.clone(),
        model: Some(request.model.clone()),
        thinking_level: request.thinking_level.clone(),
        temperature: request.temperature,
    }
}

/// Requires the dispatched request configuration to equal the current metadata authority.
/// 要求已派发请求配置与当前 metadata 权威完全一致。
pub(super) fn validate_request_against_active_config(
    active: &ActiveRequestConfig,
    requested: &ActiveRequestConfig,
) -> Result<(), DomainError> {
    ensure_api_request_field("active.provider", &active.provider, &requested.provider)?;
    ensure_api_request_field(
        "active.connection_id",
        &active.connection_id,
        &requested.connection_id,
    )?;
    ensure_api_request_field("active.model", &active.model, &requested.model)?;
    ensure_api_request_field(
        "active.thinking_level",
        &crate::v1::config::canonical_request_thinking_level(active.thinking_level.as_deref()),
        &crate::v1::config::canonical_request_thinking_level(requested.thinking_level.as_deref()),
    )?;
    ensure_api_request_field(
        "active.temperature",
        &active.temperature,
        &requested.temperature,
    )?;
    Ok(())
}

/// Compares pending request facts while ignoring their audit-only set time.
/// 比较待生效请求事实，同时忽略仅用于审计的设置时间。
fn pending_request_effectively_equal(
    left: &PendingRequestConfig,
    right: &PendingRequestConfig,
) -> bool {
    left.provider == right.provider
        && left.connection_id == right.connection_id
        && left.model == right.model
        && left.thinking_level == right.thinking_level
        && left.temperature == right.temperature
}

// Keep all mutations on the single authoritative document instance.
// 所有修改仍作用于唯一权威文档实例。
impl SessionDocument {
    /// Stages one input-bound request configuration without publishing partial owner state.
    /// 暂存一份输入绑定请求配置，且不发布任何部分 owner 状态。
    ///
    /// # Parameters / 参数
    /// `staged_metadata`, `staged_allocator`, and `staged_events` are the caller's atomic
    /// transition copies; `next_active` is the exact configuration bound to the admitted node;
    /// `turn_id` scopes an optional UI notice; `now_ms` is the transition timestamp.
    /// `staged_metadata`、`staged_allocator` 与 `staged_events` 是调用方的原子转换副本；
    /// `next_active` 是绑定到已准入节点的精确配置；`turn_id` 限定可选界面提示范围；
    /// `now_ms` 是转换时间戳。
    ///
    /// # Returns / 返回值
    /// Returns the optional request-switch notice sequence. A matching pending request is consumed;
    /// a newer different pending request remains staged for a later user boundary.
    /// 返回可选请求切换提示序号。匹配的待生效请求会被消费；更新且不同的待生效请求继续保留，
    /// 等待后续用户边界。
    pub(super) fn stage_bound_request_activation(
        &self,
        staged_metadata: &mut MetadataDocument,
        staged_allocator: &mut IdAllocator,
        staged_events: &mut Vec<ConversationEvent>,
        next_active: ActiveRequestConfig,
        turn_id: Option<TurnId>,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        let previous_active = staged_metadata.config.active_request.clone();
        let changed = previous_active != next_active;
        let emits_notice =
            changed && (previous_active.provider.is_some() || previous_active.model.is_some());
        if staged_metadata
            .config
            .pending_request
            .as_ref()
            .is_some_and(|pending| active_request_from_pending(pending) == next_active)
        {
            staged_metadata.config.pending_request = None;
        }
        staged_metadata.config.active_request = next_active.clone();
        if !emits_notice {
            return Ok(None);
        }
        self.stage_event(
            staged_allocator,
            staged_events,
            turn_id,
            EventVisibility::UiOnly,
            EventBody::RequestConfigChanged(RequestConfigChangedEvent {
                previous: previous_active,
                current: next_active,
            }),
            now_ms,
        )
        .map(Some)
    }

    /// Sets the request configuration to apply at the next admitted user or Continue boundary.
    /// 设置将在下一个已准入用户输入或 Continue 边界生效的请求配置。
    ///
    /// This mutation affects only `metadata.json`; it never appends a timeline
    /// event because no switch has occurred yet.
    /// 此修改只影响 `metadata.json`；配置尚未切换，因此绝不追加时间线事件。
    pub fn set_pending_request(
        &mut self,
        pending: PendingRequestConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("pending_request.set_at_ms", pending.set_at_ms)?;
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_pending_request_config(&pending)?;
        if self
            .metadata
            .config
            .pending_request
            .as_ref()
            .is_some_and(|current| pending_request_effectively_equal(current, &pending))
        {
            return Ok(false);
        }
        self.metadata.config.pending_request = Some(pending);
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Clears one pending request only when it still matches the caller's exact captured intent.
    /// 仅当待生效请求仍匹配调用方捕获的精确意图时清除它。
    ///
    /// # Parameters / 参数
    /// `expected` is the immutable pending projection captured before admission; `now_ms` is the
    /// metadata mutation timestamp.
    /// `expected` 是准入前捕获的不可变待生效投影；`now_ms` 是 metadata 修改时间戳。
    ///
    /// # Returns / 返回值
    /// Returns `true` only when the matching pending request was removed.
    /// 仅当匹配的待生效请求已被移除时返回 `true`。
    pub fn clear_pending_request_if_matches(
        &mut self,
        expected: &ActiveRequestConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let matches_expected = self
            .metadata
            .config
            .pending_request
            .as_ref()
            .is_some_and(|pending| active_request_from_pending(pending) == *expected);
        if !matches_expected {
            return Ok(false);
        }
        self.metadata.config.pending_request = None;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets the normalized session output-style selection in authoritative metadata.
    /// 在权威 metadata 中设置规范化后的会话输出风格选择。
    ///
    /// # Parameters
    /// 参数
    /// `style_id` is the optional catalog identity; surrounding whitespace is
    /// removed and an empty value means the global/default style. `now_ms` is
    /// the semantic mutation time.
    /// `style_id` 是可选目录身份；首尾空白会被移除，空值表示全局/默认风格；
    /// `now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the normalized selection changed.
    /// 仅在规范化选择真实变化时返回 `true`。
    pub fn set_output_style(
        &mut self,
        style_id: Option<String>,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let style_id = normalize_optional_selection(style_id);
        if self.metadata.config.selection.output_style == style_id {
            return Ok(false);
        }
        self.reject_dedicated_change("output_style")?;
        self.metadata.config.selection.output_style = style_id;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Commits the complete first-turn selection bundle as one failure-atomic metadata revision.
    /// 把完整首轮选择配置作为一次失败原子的 metadata 修订提交。
    ///
    /// # Parameters
    /// 参数
    /// `input` contains the complete output-style, Provider-tool, knowledge-library, and VMM
    /// selections; `now_ms` is the semantic mutation time.
    /// `input` 包含完整的输出风格、Provider 工具、知识库与 VMM 选择；`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when at least one exact durable selection changed. Every fallible
    /// validation completes before the document is mutated.
    /// 仅当至少一项精确持久化选择真实变化时返回 `true`；全部可能失败的校验均在修改文档前完成。
    pub fn set_session_selection_bundle(
        &mut self,
        input: SessionSelectionBundleInput,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_provider_tool_selection_config(&input.provider_tools)?;
        validate_knowledge_library_selection_config(&input.knowledge_libraries)?;

        let mut selection = self.metadata.config.selection.clone();
        selection.output_style = normalize_optional_selection(input.output_style);
        selection.provider_tools = input.provider_tools;
        selection.knowledge_libraries = input.knowledge_libraries;
        selection.vmm = input.vmm;
        if self.metadata.config.selection == selection {
            return Ok(false);
        }
        self.reject_dedicated_change("selection_bundle")?;

        self.metadata.config.selection = selection;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets the complete session-owned Provider tool selection in authoritative metadata.
    /// 在权威 metadata 中设置当前会话持有的完整 Provider 工具选择。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the fully materialized, canonically ordered selection; `now_ms` is the semantic
    /// mutation time.
    /// `config` 是已经完整物化并按规范排序的选择；`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the exact durable selection changed.
    /// 仅在精确持久化选择真实变化时返回 `true`。
    pub fn set_provider_tool_selection(
        &mut self,
        config: ProviderToolSelectionConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let config = config.marked_against(
            &self.metadata.config.selection.provider_tools,
            self.metadata.config.answer_mode.prepared.is_some(),
        );
        validate_provider_tool_selection_config(&config)?;
        if self.metadata.config.selection.provider_tools == config {
            return Ok(false);
        }
        self.metadata.config.selection.provider_tools = config;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets the complete session-owned knowledge-library selection in authoritative metadata.
    /// 在权威 metadata 中设置当前会话持有的完整知识库选择。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the canonically ordered authorized selection and `now_ms` is the semantic
    /// mutation time.
    /// `config` 是按规范排序且已经授权的选择，`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when the exact durable selection changed.
    /// 仅在精确持久化选择真实变化时返回 `true`。
    pub fn set_knowledge_library_selection(
        &mut self,
        config: KnowledgeLibrarySelectionConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        validate_knowledge_library_selection_config(&config)?;
        if self.metadata.config.selection.knowledge_libraries == config {
            return Ok(false);
        }
        self.reject_dedicated_change("knowledge_libraries")?;
        self.metadata.config.selection.knowledge_libraries = config;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Sets both session-level VMM behavior overrides in authoritative metadata.
    /// 在权威 metadata 中同时设置两项会话级 VMM 行为覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `config` contains the complete tri-state policy pair and `now_ms` is the semantic mutation
    /// time.
    /// `config` 包含完整的三态策略对，`now_ms` 是语义修改时间。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` only when either durable override changed.
    /// 仅在任一持久化覆盖真实变化时返回 `true`。
    pub fn set_vmm_session_policy(
        &mut self,
        config: VmmSessionPolicyConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        if self.metadata.config.selection.vmm == config {
            return Ok(false);
        }
        self.reject_dedicated_change("vmm")?;
        self.metadata.config.selection.vmm = config;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Replaces the session request options; they apply from the next Provider request.
    /// 替换会话请求选项；自下一次 Provider 请求起生效。
    ///
    /// Like the reasoning level these are request controls, so dedicated sessions keep them editable.
    /// 与思考档位一样属于请求控制，因此专职会话仍可修改。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the stored options changed.
    /// 仅当已存储选项变化时返回 `true`。
    pub fn set_request_options(
        &mut self,
        options: RequestOptionsConfig,
        now_ms: i64,
    ) -> Result<bool, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        for value in [&options.prompt_cache_ttl, &options.service_tier]
            .into_iter()
            .flatten()
        {
            if value.trim().is_empty() || value.trim() != value {
                return Err(DomainError::InvalidSelectionConfiguration {
                    field: "request_options",
                });
            }
        }
        if self.metadata.config.selection.request_options == options {
            return Ok(false);
        }
        self.metadata.config.selection.request_options = options;
        self.bump_revision(&[DirtyFile::Metadata], now_ms, &[]);
        Ok(true)
    }

    /// Activates pending configuration at an explicit admitted execution boundary.
    /// 在明确已准入的执行边界激活待生效配置。
    ///
    /// `turn_id` explicitly scopes the display fact when the boundary belongs
    /// to an open turn; `None` records a session-level boundary. The method
    /// validates every fallible condition before mutating metadata, so active
    /// replacement, pending removal, and the optional display event form one
    /// all-or-nothing document transition.
    /// 当边界属于开放回合时，`turn_id` 显式限定显示事实；`None` 记录会话级边界。
    /// 该方法在修改 metadata 前校验所有可能失败的条件，因此 active 替换、pending
    /// 移除和可选显示事件构成一次全有或全无的文档转换。
    pub fn promote_pending_request(
        &mut self,
        turn_id: Option<TurnId>,
        now_ms: i64,
    ) -> Result<Option<Seq>, DomainError> {
        validate_timestamp("updated_at_ms", now_ms)?;
        let pending = self
            .metadata
            .config
            .pending_request
            .as_ref()
            .cloned()
            .ok_or_else(|| DomainError::reference("pending_request", 0))?;
        if self.active_api.is_some() {
            return Err(DomainError::PromoteWhileApiOpen);
        }
        let next_active = active_request_from_pending(&pending);
        let previous_active = self.metadata.config.active_request.clone();
        let changed = previous_active != next_active;
        // Initial selection initializes the route; only an established previous route can produce a switch notice.
        // 首次选择只初始化路线；只有已建立的上一条路线才能产生切换提示。
        let emits_notice =
            changed && (previous_active.provider.is_some() || previous_active.model.is_some());
        if changed {
            if let Some(expected_turn_id) = turn_id {
                let actual_turn_id = self.require_open_turn()?.turn_id;
                if expected_turn_id != actual_turn_id {
                    return Err(DomainError::TurnIdentityMismatch {
                        expected: expected_turn_id.value(),
                        actual: actual_turn_id.value(),
                    });
                }
            }
            if emits_notice {
                self.ensure_transition_capacity(1, 0)?;
            }
        }
        let mut staged_allocator = self.id_allocator;
        let mut staged_events = Vec::with_capacity(usize::from(emits_notice));
        let mut staged_metadata = self.metadata.clone();
        let notice_seq = self.stage_bound_request_activation(
            &mut staged_metadata,
            &mut staged_allocator,
            &mut staged_events,
            next_active,
            turn_id,
            now_ms,
        )?;
        self.metadata = staged_metadata;
        self.id_allocator = staged_allocator;
        self.events.extend(staged_events);
        let changed_event_seqs = notice_seq.into_iter().collect::<Vec<_>>();
        let dirty_files: &[DirtyFile] = if changed_event_seqs.is_empty() {
            &[DirtyFile::Metadata]
        } else {
            &[DirtyFile::Metadata, DirtyFile::ActiveTail]
        };
        self.bump_revision(dirty_files, now_ms, &changed_event_seqs);
        Ok(changed_event_seqs.first().copied())
    }
}

/// Converts one pending request into the exact active configuration it represents.
/// 把一份待生效请求转换为它所表示的精确活动配置。
///
/// # Parameters / 参数
/// `pending` is the session-owned request intent already validated by its setter.
/// `pending` 是已经由 setter 校验的会话自有请求意图。
///
/// # Returns / 返回值
/// Returns the corresponding active configuration without normalization or fallback.
/// 返回对应活动配置，不执行规范化或回退。
pub(super) fn active_request_from_pending(pending: &PendingRequestConfig) -> ActiveRequestConfig {
    ActiveRequestConfig {
        provider: pending.provider.clone(),
        connection_id: pending.connection_id.clone(),
        model: pending.model.clone(),
        thinking_level: pending.thinking_level.clone(),
        temperature: pending.temperature,
    }
}
