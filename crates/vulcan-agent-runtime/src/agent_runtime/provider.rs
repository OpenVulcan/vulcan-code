//! Provider/model runtime blueprint resolution used by the compose execution layer.
//! compose 执行层使用的 Provider/Model 运行时蓝图解析逻辑。

use crate::agent_runtime::provider_reasoning::{
    ProviderRuntimeReasoningResolution, validate_exact_wire_reasoning_effort,
};
use crate::provider_config::ReasoningEffortLevel;

/// Current persisted schema version for session provider-route lease identities.
/// 当前会话供应商路由租约身份的持久化 Schema 版本。
const SESSION_PROVIDER_ROUTE_LEASE_SCHEMA_VERSION: u32 = 1;

/// Stable diagnostic emitted when a persisted route no longer matches the installed Manifest.
/// 当持久化路由不再匹配已安装 Manifest 时输出的稳定诊断。
pub const PROVIDER_MANIFEST_ROUTE_CHANGED_MESSAGE: &str = "The provider manifest changed after this session activated; reselect the model to adopt the new route.";

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
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID => "Anthropic Messages",
            vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => "Anthropic Messages + Video",
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID => "OpenAI Chat Completions",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID => "OpenAI Responses",
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
            .unwrap_or_else(|| String::from("resolved automatically by the runtime"))
    }

    /// Returns whether the active model accepts explicit temperature overrides on its route.
    /// 返回当前激活模型在其路由上是否接受显式温度覆盖。
    pub fn supports_temperature(&self) -> bool {
        self.public_capabilities
            .model_parameters
            .iter()
            .any(|definition| {
                definition.id == vulcan_provider_adapters::SAMPLING_TEMPERATURE_PARAMETER_ID
                    && definition.applies_to(&self.protocol_profile_id, &self.model_id)
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
        manifest_revision: selection.protocol_binding.manifest_revision,
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
            return Err(String::from(PROVIDER_MANIFEST_ROUTE_CHANGED_MESSAGE));
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

    provider_blueprint_from_selection(selection, requested_reasoning_effort)
}

/// Freezes an explicit Git summary target without reading or mutating conversation leases.
/// 冻结明确的 Git 总结目标，不读取或修改会话租约。
///
/// `runtime` owns the current catalog; provider/model IDs are the validated summary setting.
/// `runtime` 持有当前目录；供应商和模型标识来自已校验的总结设置。
/// Returns the frozen blueprint or an explicit target/reasoning error, with no fallback.
/// 返回冻结蓝图或明确的目标、思考档位错误，不尝试替代目标。
pub(crate) fn resolve_git_commit_summary_provider_blueprint(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    provider_id: &str,
    model_id: &str,
    requested_reasoning_effort: ReasoningEffortLevel,
) -> Result<ProviderRuntimeBlueprint, String> {
    // configuration and selection freeze one model-level route under current eligibility rules.
    // configuration 与 selection 按当前准入规则冻结一条模型级路由。
    let configuration = runtime.configuration_snapshot()?;
    let selection = runtime.resolve_model_selection(&configuration, provider_id, model_id)?;
    provider_blueprint_from_selection(selection, requested_reasoning_effort)
}

/// Projects one already resolved immutable selection into execution and accounting facts.
/// 将已解析的不可变选择投影为执行和统计事实。
///
/// `selection` owns the exact route; `requested_reasoning_effort` is validated without substitution.
/// `selection` 持有精确路由；`requested_reasoning_effort` 经过校验且不会被替换。
/// Returns a complete execution blueprint, or the exact reasoning validation error.
/// 返回完整执行蓝图，或精确的思考档位校验错误。
pub(crate) fn provider_blueprint_from_selection(
    selection: vulcan_provider_adapters::ResolvedPublicAiSelection,
    requested_reasoning_effort: ReasoningEffortLevel,
) -> Result<ProviderRuntimeBlueprint, String> {
    // protocolId is read from the exact immutable selection, never resolved again.
    // protocolId 从精确的不可变选择读取，绝不再次解析。
    let protocol_id = selection.route.protocol_route.protocol_profile_id.as_str();
    // Budgeting and transport dispatch observe the same immutable request blueprint.
    // 预算与传输分派观察同一份不可变请求蓝图。
    let mut capabilities = selection.model.capabilities.clone();
    // Apply the shared host default before exposing the blueprint to budgeting and transport.
    // 在将运行时蓝图交给预算与传输层前应用共享宿主默认策略。
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut capabilities);
    // Provider resolution validates session authority but never substitutes another reasoning level.
    // Provider 解析只校验会话权威值，绝不替换成其他思考档位。
    validate_exact_wire_reasoning_effort(&capabilities, requested_reasoning_effort).map_err(
        |error| {
            format!(
                "Model `{}` rejected session reasoning: {error}",
                selection.model.id
            )
        },
    )?;
    Ok(ProviderRuntimeBlueprint {
        provider_id: selection.route.provider.id.clone(),
        service_entry_id: selection.route.entry.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        provider_name: selection.route.provider.display_name.clone(),
        runtime_label: String::from("Vulcan Public AI Contract"),
        model_id: selection.model.id.clone(),
        model_name: selection.model.display_name.clone(),
        protocol_profile_id: protocol_id.to_string(),
        manifest_revision: selection.protocol_binding.manifest_revision,
        frozen_selection: Some(selection.clone()),
        route_revision: selection.route_revision.clone(),
        secondary_route: selection.route.secondary_route.clone(),
        public_capabilities: capabilities,
        base_url: Some(selection.route.entry.base_url.clone()),
        supports_remote_model_refresh: true,
        credential_source: selection.route.connection.display_name.clone(),
        reasoning: ProviderRuntimeReasoningResolution {
            requested_effort: requested_reasoning_effort,
            source_label: String::from("Public contract"),
            note: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an independent summary target never replaces the live conversation lease.
    /// 验证独立总结目标不会替换正在使用的会话租约。
    /// Uses current signed OpenRouter bindings and local catalogs; performs no network or secret reads.
    /// 使用当前签名的 OpenRouter 绑定和本地目录，不进行网络或秘密读取。
    #[test]
    fn git_summary_target_is_independent_from_conversation_lease() {
        use vulcan_provider_adapters::{
            ActiveAiModelSelection, CredentialConnection, CredentialMode, PersistedAiConfiguration,
            ProtocolModelVariant, PublicAiRuntime, parse_openrouter_model_catalog,
        };
        // runtime and configuration are isolated process-local authorities for two declared models.
        // runtime 与 configuration 是两个已声明模型的进程内隔离权威。
        let runtime = PublicAiRuntime::unavailable().expect("isolated runtime");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime.providers().unwrap(),
                CredentialConnection {
                    id: String::from("summary-test"),
                    provider_id: String::from("openrouter"),
                    service_entry_id: String::from("public-api"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: CredentialMode::Token,
                    credential_id: String::from("summary-credential"),
                    display_name: String::from("Summary test"),
                    secret_reference: String::from("unused-summary-secret"),
                    management_secret_reference: None,
                    service_origin_override: None,
                },
            )
            .expect("isolated connection");
        // catalog uses the documented provider parser, not hand-built resolved selections.
        // catalog 使用正式供应商解析器，不手工构造解析后选择。
        let mut catalog = parse_openrouter_model_catalog("summary-test", 1,
            r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Conversation","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},{"id":"openai/gpt-5.6-terra","name":"Summary","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#
        ).expect("documented source catalog");
        for model in &mut catalog.models {
            for profile in [
                "openai-chat-completions.v1",
                "openai-responses.v1",
                "anthropic-messages.v1",
            ] {
                model.protocol_variants.insert(
                    profile.to_string(),
                    ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: Default::default(),
                        discovery_signature: String::from("summary-test-catalog"),
                        catalog_generation: catalog.generation,
                    },
                );
            }
            configuration.set_model_enabled("openrouter", &model.id, true);
        }
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("summary-test"),
        });
        runtime
            .stage_source_catalog_for_testing(catalog)
            .expect("source catalog");
        runtime
            .replace_configuration(configuration.clone())
            .expect("configuration");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("effective catalog");
        // before captures the exact original route before the independent request is prepared.
        // before 记录独立请求准备之前的精确原始路由。
        let before = resolve_public_ai_provider_blueprint(
            &runtime,
            "conversation-owner",
            None,
            ReasoningEffortLevel::Off,
            None,
            None,
            None,
        )
        .expect("conversation lease");
        // summary must freeze the selected model while preserving original session and global state.
        // summary 必须冻结选中模型，同时保留原会话与全局状态。
        let summary = resolve_git_commit_summary_provider_blueprint(
            &runtime,
            "openrouter",
            "openai/gpt-5.6-terra",
            ReasoningEffortLevel::Off,
        )
        .expect("independent summary");
        assert_eq!(summary.model_id, "openai/gpt-5.6-terra");
        assert_eq!(
            summary.frozen_selection.as_ref().unwrap().model.id,
            "openai/gpt-5.6-terra"
        );
        assert!(
            resolve_git_commit_summary_provider_blueprint(
                &runtime,
                "openrouter",
                "missing-model",
                ReasoningEffortLevel::Off
            )
            .is_err()
        );
        assert_eq!(
            runtime
                .session_selection_lease("conversation-owner")
                .unwrap()
                .unwrap()
                .route_revision,
            before.route_revision
        );
        assert_eq!(
            runtime.configuration_snapshot().unwrap().active,
            configuration.active
        );
        // following resolves the validated conversation model without changing its established lease.
        // following 解析已校验的会话模型，同时不改变其既有租约。
        let following = resolve_git_commit_summary_provider_blueprint(
            &runtime,
            &before.provider_id,
            &before.model_id,
            ReasoningEffortLevel::Off,
        )
        .unwrap();
        assert_eq!(following.model_id, before.model_id);
    }

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

    /// Verifies a persisted route lease from an unknown schema version is refused before it is used.
    /// 验证来自未知契约版本的持久化路由租约在被使用之前即被拒绝。
    ///
    /// # Notes
    /// 说明
    /// The version check precedes every runtime lookup, so an unavailable runtime is enough to prove the
    /// refusal happens on the version boundary itself and never degrades into route reuse. A future
    /// version is used rather than a past one because this contract is unreleased and has no historical
    /// form to remain compatible with.
    /// 版本判定先于任何运行时查询，因此不可用的运行时足以证明拒绝发生在版本边界本身、而不会退化为
    /// 路由复用。这里取未来版本而非过去版本，因为本契约尚未发布、没有需要兼容的历史形态。
    #[test]
    fn persisted_route_lease_with_unknown_schema_version_is_refused() {
        use vulcan_provider_adapters::PublicAiRuntime;
        let runtime = PublicAiRuntime::unavailable().expect("isolated runtime");
        let mut lease = test_route_lease_identity();
        lease.schema_version = SESSION_PROVIDER_ROUTE_LEASE_SCHEMA_VERSION + 1;
        let error = resolve_public_ai_provider_blueprint(
            &runtime,
            "session-with-unknown-lease-version",
            Some(&lease),
            ReasoningEffortLevel::Off,
            None,
            None,
            None,
        )
        .expect_err("an unknown route lease schema version must be refused");
        assert!(
            error.contains("unsupported schema version"),
            "the refusal must name the version boundary: {error}"
        );
        // The recorded lease must not have been accepted, reused or rewritten by the refusal.
        // 该拒绝不得接纳、复用或改写被记录下来的租约。
        assert!(
            runtime
                .session_selection_lease("session-with-unknown-lease-version")
                .expect("lease lookup must remain readable")
                .is_none(),
            "refusing an unknown lease version must not install a route lease"
        );
    }
}
