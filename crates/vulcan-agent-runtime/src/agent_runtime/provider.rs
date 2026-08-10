//! Provider/model runtime blueprint resolution used by the compose execution layer.
//! compose 执行层使用的 Provider/Model 运行时蓝图解析逻辑。

use crate::agent_runtime::provider_reasoning::{
    ProviderRuntimeReasoningResolution, resolve_effective_reasoning_effort,
};
use crate::provider_config::ReasoningEffortLevel;

/// Current persisted schema version for session provider-route lease identities.
/// 当前会话供应商路由租约身份的持久化 Schema 版本。
const SESSION_PROVIDER_ROUTE_LEASE_SCHEMA_VERSION: u32 = 1;

/// Resolved provider/model runtime blueprint used by execution preparation.
/// 执行准备阶段使用的已解析 Provider/Model 运行时蓝图。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeBlueprint {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Exact provider service-entry identifier selected for execution.
    /// 执行选择的精确供应商服务入口标识。
    pub service_entry_id: String,
    /// Exact credential connection identifier selected for execution.
    /// 执行选择的精确凭据连接标识。
    pub connection_id: String,
    /// Human-readable provider name.
    /// 人类可读的供应商名称。
    pub provider_name: String,
    /// Human-readable runtime-family label used by diagnostics.
    /// 诊断使用的人类可读运行时族标签。
    pub runtime_label: String,
    /// Stable model identifier.
    /// 稳定模型标识。
    pub model_id: String,
    /// Human-readable model name.
    /// 人类可读的模型名称。
    pub model_name: String,
    /// Exact provider-neutral wire protocol profile selected for this immutable execution.
    /// 为本次不可变执行选定的精确供应商无关 Wire 协议 Profile。
    pub protocol_profile_id: String,
    /// Manifest revision frozen with this execution.
    /// 与本次执行一起冻结的清单修订号。
    pub manifest_revision: u64,
    /// Complete non-secret selection frozen at the activation boundary; execution re-reads
    /// only the secret and never re-resolves the active catalog (plan §6.3/§R4). `None`
    /// only for preview/test blueprints that never enter provider dispatch.
    /// 在激活边界冻结的完整非秘密选择；执行阶段只重读秘密，绝不重新解析活动目录
    /// （方案 §6.3/§R4）。仅预览/测试蓝图（绝不进入供应商分派）为 `None`。
    pub frozen_selection: Option<vulcan_provider_adapters::ResolvedPublicAiSelection>,
    /// Content-addressed revision of the exact route snapshot selected for this execution.
    /// 为本次执行选定的精确路由快照内容寻址修订号。
    pub route_revision: String,
    /// Exact provider-internal secondary route selected for this immutable execution.
    /// 为本次不可变执行选定的精确供应商内部二级路由。
    pub secondary_route: vulcan_provider_adapters::SecondaryRouteSelection,
    /// Exact public model capability snapshot captured for this immutable execution.
    /// 为本次不可变执行捕获的精确公共模型能力快照。
    pub public_capabilities: vulcan_ai_contract::ModelCapabilities,
    /// Resolved base URL when one is available.
    /// 当存在时的已解析接口基址。
    pub base_url: Option<String>,
    /// Whether the provider can actively refresh remote models.
    /// 该供应商是否可以主动刷新远端模型列表。
    pub supports_remote_model_refresh: bool,
    /// Human-readable credential source summary.
    /// 人类可读的凭据来源摘要。
    pub credential_source: String,
    /// Shared reasoning-resolution result derived from model overrides, provider overrides, and runtime defaults.
    /// 由模型覆盖、供应商覆盖与运行时默认值共同解析出的共享推理控制结果。
    pub reasoning: ProviderRuntimeReasoningResolution,
}

impl ProviderRuntimeBlueprint {
    /// Returns the canonical provider API family derived from the resolved runtime kind.
    /// 返回从已解析运行时类型派生出的规范 Provider 接口族。
    /// Returns the stable API-family key used by diagnostics and request previews.
    /// 返回供诊断与请求预览复用的稳定接口族键值。
    pub fn protocol_profile_key(&self) -> &str {
        self.protocol_profile_id.as_str()
    }

    /// Returns the human-readable API-family label used by diagnostics surfaces.
    /// 返回供诊断界面使用的人类可读接口族标签。
    pub fn protocol_profile_label(&self) -> &'static str {
        match self.protocol_profile_id.as_str() {
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => "Anthropic Messages",
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
                "Anthropic Messages + Video"
            }
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => "OpenAI Chat Completions",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID => "OpenAI Responses",
            _ => "Unregistered protocol profile",
        }
    }

    /// Returns a compact stable runtime-kind key used by diagnostics and future factories.
    /// 返回供诊断与后续工厂使用的紧凑稳定运行时键值。
    pub const fn runtime_kind_key(&self) -> &'static str {
        "public-ai-contract"
    }

    /// Returns a compact endpoint summary rendered in execution diagnostics.
    /// 返回展示在执行诊断中的紧凑接口摘要。
    pub fn endpoint_summary(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| String::from("由运行时自动解析"))
    }

    /// Returns whether the active model accepts direct tool exposure for the current turn.
    /// 返回当前激活模型是否接受本轮直接暴露工具。
    pub fn supports_tool_call(&self) -> bool {
        self.public_capabilities.tool_calling.is_supported()
    }

    /// Returns whether the active model accepts explicit temperature overrides.
    /// 返回当前激活模型是否接受显式温度覆盖。
    pub fn supports_temperature(&self) -> bool {
        self.public_capabilities
            .model_parameters
            .iter()
            .any(|definition| {
                definition.id == vulcan_provider_adapters::SAMPLING_TEMPERATURE_PARAMETER_ID
            })
    }

    /// Builds the persisted non-secret route identity for this prepared provider blueprint.
    /// 为当前已准备供应商蓝图构建持久化非秘密路由身份。
    ///
    /// # Returns
    /// 返回值
    /// The exact lease identity when this blueprint owns a dispatchable frozen selection.
    /// 当前蓝图拥有可分派冻结选择时返回精确租约身份。
    pub fn session_provider_route_lease_identity(
        &self,
    ) -> Option<vulcan_workbench_model::SessionProviderRouteLeaseIdentity> {
        self.frozen_selection
            .as_ref()
            .map(provider_route_lease_identity)
    }
}

/// Projects one complete non-secret runtime selection into its persisted session identity.
/// 将一份完整非秘密运行时选择投影为持久化会话身份。
///
/// # Parameters
/// 参数
/// `selection` is the exact immutable route snapshot leased by the active session.
/// `selection` 是活动会话租用的精确不可变路由快照。
///
/// # Returns
/// 返回值
/// A stable identity that can validate restart restoration without persisting secrets.
/// 返回可在不持久化秘密的情况下校验重启恢复的稳定身份。
pub fn provider_route_lease_identity(
    selection: &vulcan_provider_adapters::ResolvedPublicAiSelection,
) -> vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
    vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
        schema_version: SESSION_PROVIDER_ROUTE_LEASE_SCHEMA_VERSION,
        provider_id: selection.route.provider.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        model_id: selection.model.id.clone(),
        protocol_profile_id: selection.route.protocol_route.protocol_profile_id.clone(),
        manifest_revision: selection
            .protocol_binding
            .as_ref()
            .map(|binding| binding.manifest_revision)
            .unwrap_or(0),
        manifest_generation: selection.manifest_generation,
        catalog_revision: selection.catalog_revision.clone(),
        route_revision: selection.route_revision.clone(),
    }
}

/// Compares one persisted route lease with one freshly resolved content identity.
/// 比较一份持久化路由租约与一份重新解析出的内容身份。
///
/// # Parameters
/// 参数
/// `expected` is the persisted non-secret lease and `actual` is resolved from the current
/// immutable provider generation.
/// `expected` 是持久化的非秘密租约，`actual` 从当前不可变供应商代际解析得到。
///
/// # Returns
/// 返回值
/// `true` only when the target identity and content-addressed execution-route revision match.
/// Manifest/catalog revisions remain diagnostic metadata because display-only catalog changes
/// must not invalidate an otherwise identical frozen execution route (original plan §17.7).
/// 仅当目标身份与内容寻址的执行路由修订一致时返回 `true`。Manifest/目录修订只作为诊断元数据，
/// 因为纯展示目录变化不得让其他方面完全相同的冻结执行路由失效（原始方案 §17.7）。
pub fn provider_route_lease_identity_matches(
    expected: &vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
    actual: &vulcan_workbench_model::SessionProviderRouteLeaseIdentity,
) -> bool {
    expected.schema_version == actual.schema_version
        && expected.provider_id == actual.provider_id
        && expected.connection_id == actual.connection_id
        && expected.model_id == actual.model_id
        && expected.protocol_profile_id == actual.protocol_profile_id
        && expected.route_revision == actual.route_revision
}

/// Validates that one leased selection belongs to the exact requested session target.
/// 校验一个租约选择是否属于精确的会话请求目标。
///
/// # Parameters
/// 参数
/// The optional provider/model/connection identifiers are the only supported request override
/// shape and `selection` is the existing immutable session lease.
/// 可选供应商、模型和连接标识是唯一受支持的请求覆盖形态，`selection` 是既有不可变会话租约。
///
/// # Returns
/// 返回值
/// `Ok(())` for the same target, or an explicit stale-lease error requiring reselection.
/// 目标一致时返回 `Ok(())`，否则返回要求重新选择的显式过期租约错误。
fn validate_session_lease_target(
    selection: &vulcan_provider_adapters::ResolvedPublicAiSelection,
    provider_id_override: Option<&str>,
    model_id_override: Option<&str>,
    connection_id_override: Option<&str>,
) -> Result<(), String> {
    if provider_id_override.is_some_and(|provider_id| provider_id != selection.route.provider.id)
        || model_id_override.is_some_and(|model_id| model_id != selection.model.id)
        || connection_id_override
            .is_some_and(|connection_id| connection_id != selection.route.connection.id)
    {
        return Err(String::from(
            "The active session provider route lease does not match the requested model target; reselect the model before retrying.",
        ));
    }
    Ok(())
}

/// Resolves the new public AI selection into the temporary controller blueprint shape.
/// 将新的公共 AI 选择解析为临时控制器蓝图形态。
///
/// # Parameters
/// 参数
/// `runtime` owns the in-memory catalog, `config` owns the active selection, and
/// `requested_reasoning_effort` is retained only as a host diagnostic preference.
/// `runtime` 持有内存目录，`config` 持有当前选择，`requested_reasoning_effort`
/// 仅作为宿主诊断偏好保留。
///
/// # Returns
/// 返回值
/// A controller compatibility snapshot whose network dispatch is still owned by the public adapter.
/// 返回网络分派仍由公共适配器持有的控制器兼容快照。
pub fn resolve_public_ai_provider_blueprint(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    session_id: &str,
    expected_route_lease: Option<&vulcan_workbench_model::SessionProviderRouteLeaseIdentity>,
    requested_reasoning_effort: ReasoningEffortLevel,
    provider_id_override: Option<&str>,
    model_id_override: Option<&str>,
    connection_id_override: Option<&str>,
) -> Result<ProviderRuntimeBlueprint, String> {
    if session_id.trim().is_empty() {
        return Err(String::from(
            "Public AI execution requires a non-empty session identifier for route leasing.",
        ));
    }
    if expected_route_lease.is_some_and(|identity| {
        identity.schema_version != SESSION_PROVIDER_ROUTE_LEASE_SCHEMA_VERSION
    }) {
        return Err(String::from(
            "The persisted session provider route lease uses an unsupported schema version; reselect the model before retrying.",
        ));
    }

    // existingLease is keyed only by the session owner. This is the uniqueness boundary that
    // prevents two concurrent first turns with different targets from installing two routes.
    // existingLease 仅以会话所有者为键。这是阻止两个不同目标的并发首轮安装两条路由的
    // 唯一性边界。
    let existing_lease = runtime.session_selection_lease(session_id)?;
    // selection reuses the established route or captures one candidate from exactly one current generation.
    // selection 复用已建立路由，或从精确的单个当前代际捕获一个候选。
    let selection = if let Some(selection) = existing_lease {
        validate_session_lease_target(
            &selection,
            provider_id_override,
            model_id_override,
            connection_id_override,
        )?;
        if expected_route_lease.is_some_and(|identity| {
            !provider_route_lease_identity_matches(
                identity,
                &provider_route_lease_identity(&selection),
            )
        }) {
            return Err(String::from(
                "The in-memory session provider route does not match its persisted identity; reselect the model before retrying.",
            ));
        }
        selection
    } else {
        // configuration is read once for the candidate and never consulted again after leasing.
        // configuration 仅为候选读取一次，租约建立后绝不再次查询。
        let configuration = runtime.configuration_snapshot()?;
        // candidate resolves through the current immutable provider generation exactly once.
        // candidate 仅通过当前不可变供应商代际解析一次。
        let candidate = match (
            provider_id_override,
            model_id_override,
            connection_id_override,
        ) {
            (Some(provider_id), Some(model_id), Some(connection_id)) => runtime
                .resolve_connection_model_selection(
                    &configuration,
                    connection_id,
                    provider_id,
                    model_id,
                )?,
            (Some(provider_id), Some(model_id), None) => {
                runtime.resolve_model_selection(&configuration, provider_id, model_id)?
            }
            (None, None, None) => runtime.resolve_active_selection(&configuration)?,
            _ => {
                return Err(String::from(
                    "Public AI request overrides require provider_id and model_id; connection_id is optional only with both.",
                ));
            }
        };
        // candidateIdentity proves restart restoration is safe before the candidate enters memory.
        // candidateIdentity 在候选进入内存前证明重启恢复安全。
        let candidate_identity = provider_route_lease_identity(&candidate);
        if expected_route_lease.is_some_and(|identity| {
            !provider_route_lease_identity_matches(identity, &candidate_identity)
        }) {
            return Err(String::from(
                "The provider manifest changed after this session activated; reselect the model to adopt the new route.",
            ));
        }
        // leasedSelection preserves a concurrently installed winner instead of replacing it.
        // leasedSelection 保留并发安装的胜出者，而不是将其替换。
        let leased_selection =
            runtime.lease_session_selection(session_id.to_string(), candidate)?;
        validate_session_lease_target(
            &leased_selection,
            provider_id_override,
            model_id_override,
            connection_id_override,
        )?;
        if expected_route_lease.is_some_and(|identity| {
            !provider_route_lease_identity_matches(
                identity,
                &provider_route_lease_identity(&leased_selection),
            )
        }) {
            return Err(String::from(
                "The provider manifest changed while restoring this session route; reselect the model before retrying.",
            ));
        }
        leased_selection
    };

    // protocolId is read from the exact immutable selection leased above.
    // protocolId 从上方租用的精确不可变选择读取。
    let protocol_id = selection.route.protocol_route.protocol_profile_id.as_str();
    // Apply validated local narrowing to the immutable request blueprint so budgeting and
    // transport dispatch observe the same limits.
    // 将已校验的本地收窄应用到不可变请求蓝图，确保预算与传输分派观察到相同限制。
    let mut capabilities = selection.model.capabilities.clone();
    // Apply the shared host default before exposing the blueprint to budgeting and transport.
    // 在将运行时蓝图交给预算与传输层前应用共享宿主默认策略。
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut capabilities);
    if let Some(context_limit) = selection.overrides.context_limit {
        capabilities.context_limit = Some(context_limit);
    }
    if let Some(output_limit) = selection.overrides.output_limit {
        capabilities.output_limit = Some(output_limit);
    }
    // effectiveReasoningEffort resolves the same concrete default shown by the workbench after the exact model is known.
    // effectiveReasoningEffort 在精确模型确定后解析工作台展示的同一具体默认档位。
    let effective_reasoning_effort =
        resolve_effective_reasoning_effort(&capabilities, Some(requested_reasoning_effort));
    // reasoningNote explains only an incompatible explicit non-sentinel preference; an absent Off sentinel is a normal default path.
    // reasoningNote 仅解释不兼容的显式非哨兵偏好；缺省 Off 哨兵属于正常默认路径。
    let reasoning_note = (requested_reasoning_effort != ReasoningEffortLevel::Off
        && requested_reasoning_effort != effective_reasoning_effort)
        .then(|| {
            format!(
                "Requested reasoning effort `{}` is unsupported by model `{}`; using `{}`.",
                requested_reasoning_effort.compact_label(),
                selection.model.id,
                effective_reasoning_effort.compact_label()
            )
        });
    Ok(ProviderRuntimeBlueprint {
        provider_id: selection.route.provider.id.clone(),
        service_entry_id: selection.route.entry.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        provider_name: selection.route.provider.display_name.clone(),
        runtime_label: String::from("Vulcan Public AI Contract"),
        model_id: selection.model.id.clone(),
        model_name: selection.model.display_name.clone(),
        protocol_profile_id: protocol_id.to_string(),
        manifest_revision: selection
            .protocol_binding
            .as_ref()
            .map(|binding| binding.manifest_revision)
            .unwrap_or(0),
        frozen_selection: Some(selection.clone()),
        route_revision: selection.route_revision.clone(),
        secondary_route: selection.route.secondary_route.clone(),
        public_capabilities: capabilities,
        base_url: Some(selection.route.entry.base_url.clone()),
        supports_remote_model_refresh: true,
        credential_source: selection.route.connection.display_name.clone(),
        reasoning: ProviderRuntimeReasoningResolution {
            requested_effort: effective_reasoning_effort,
            source_label: String::from("公共契约"),
            note: reasoning_note,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one deterministic persisted route identity for comparison tests.
    /// 为比较测试构建一份确定性的持久化路由身份。
    ///
    /// # Returns
    /// 返回值
    /// A complete non-secret route identity with stable target and route fields.
    /// 返回目标字段与路由字段稳定的完整非秘密路由身份。
    fn test_route_lease_identity() -> vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
        vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
            schema_version: SESSION_PROVIDER_ROUTE_LEASE_SCHEMA_VERSION,
            provider_id: String::from("deepseek"),
            connection_id: String::from("deepseek-primary"),
            model_id: String::from("deepseek-chat"),
            protocol_profile_id: String::from("openai-chat-completions.v1"),
            manifest_revision: 7,
            manifest_generation: 11,
            catalog_revision: String::from("catalog-a"),
            route_revision: String::from("route-a"),
        }
    }

    /// Verifies diagnostic manifest and catalog drift does not invalidate an identical route.
    /// 验证诊断性的 Manifest 与目录漂移不会让相同执行路由失效。
    #[test]
    fn route_lease_restore_ignores_diagnostic_revision_drift() {
        let expected = test_route_lease_identity();
        let mut actual = expected.clone();
        actual.manifest_revision += 1;
        actual.manifest_generation += 1;
        actual.catalog_revision = String::from("catalog-b");

        assert!(provider_route_lease_identity_matches(&expected, &actual));
    }

    /// Verifies any target or content-addressed execution-route drift blocks restoration.
    /// 验证任意目标或内容寻址执行路由漂移都会阻止恢复。
    #[test]
    fn route_lease_restore_rejects_execution_identity_drift() {
        let expected = test_route_lease_identity();
        for mut actual in [
            {
                let mut identity = expected.clone();
                identity.provider_id = String::from("other-provider");
                identity
            },
            {
                let mut identity = expected.clone();
                identity.connection_id = String::from("other-connection");
                identity
            },
            {
                let mut identity = expected.clone();
                identity.model_id = String::from("other-model");
                identity
            },
            {
                let mut identity = expected.clone();
                identity.protocol_profile_id = String::from("openai-responses.v1");
                identity
            },
            {
                let mut identity = expected.clone();
                identity.route_revision = String::from("route-b");
                identity
            },
        ] {
            actual.manifest_revision += 1;
            assert!(!provider_route_lease_identity_matches(&expected, &actual));
        }
    }
}
