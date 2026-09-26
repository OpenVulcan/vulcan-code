//! Model-level protocol binding resolution.
//! 模型级协议绑定解析。
//!
//! The binding resolver is the single authority for "which protocol executes this model".
//! System providers resolve exclusively from the compiled manifest (plan §13.2); the legacy
//! connection `selected_protocol` is never consulted for system bindings.
//! 绑定解析器是"该模型由哪个协议执行"的唯一权威。系统供应商完全从已编译清单解析（方案 §13.2）；
//! 旧版连接 `selected_protocol` 绝不参与系统绑定。

use super::{ResolvedProviderManifest, UnknownModelPolicy};
use std::collections::BTreeSet;
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Source of one model protocol binding.
/// 单个模型协议绑定的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModelProtocolBindingSource {
    /// Resolved from a system manifest rule or provider default.
    /// 从系统清单规则或供应商默认解析。
    SystemManifest,
    /// Explicit user choice for an additional system model on a verified manifest route.
    /// 用户为系统供应商新增模型在已验证清单路由上作出的显式选择。
    SystemUser,
    /// Explicit user choice for a custom provider model.
    /// 自定义供应商模型的显式用户选择。
    CustomUser,
    /// Auto-resolved custom provider default at a configuration activation boundary.
    /// 配置激活边界自动解析的自定义供应商默认。
    CustomAutoDefault,
}

/// One immutable model-level protocol binding.
/// 一个不可变的模型级协议绑定。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelProtocolBinding {
    /// The unique protocol profile that executes this model.
    /// 执行该模型的唯一协议 Profile。
    pub protocol_profile_id: String,
    /// Where this binding came from.
    /// 此绑定的来源。
    pub source: ModelProtocolBindingSource,
    /// Manifest revision this binding was resolved from; zero for custom sources.
    /// 解析此绑定使用的清单修订号；自定义来源为零。
    pub manifest_revision: u64,
    /// Rule identifier that produced the binding; empty for provider-default bindings.
    /// 产生绑定的规则标识；供应商默认绑定为空。
    pub rule_id: String,
    /// Closed Codex behavior frozen from the exact matched system rule.
    /// 从精确命中系统规则冻结的封闭 Codex 行为。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_behavior: Option<super::CodexModelBehavior>,
}

/// Why one model has no executable protocol binding.
/// 一个模型无可执行协议绑定的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelProtocolBindingError {
    /// The requested service entry has no compiled model defaults.
    /// 请求的服务入口没有已编译模型默认值。
    UnknownServiceEntry {
        /// Exact missing service-entry identifier.
        /// 精确缺失的服务入口标识。
        service_entry_id: String,
    },
    /// No rule matched and `declared_rule_required` forbids the provider default.
    /// 无规则命中且 `declared_rule_required` 禁止使用供应商默认。
    /// The exact model identity that cannot bind.
    /// 无法绑定的精确模型身份。
    Missing {
        /// The exact model identity that cannot bind.
        /// 无法绑定的精确模型身份。
        model_id: String,
    },
    /// A rule matched but declares no selected protocol.
    /// 规则命中但未声明生效协议。
    /// The exact model identity and the rule that declared no protocol.
    /// 未声明协议的精确模型身份与规则。
    NoSelectedProtocol {
        /// The exact model identity.
        /// 精确模型身份。
        model_id: String,
        /// The rule that declared no selected protocol.
        /// 未声明生效协议的规则。
        rule_id: String,
    },
    /// No rule matched, the policy allows the default, but no default is declared.
    /// 无规则命中且策略允许默认，但未声明默认协议。
    /// The exact model identity that has no declared default protocol.
    /// 无声明默认协议的精确模型身份。
    NoDefaultProtocol {
        /// The exact model identity that has no declared default protocol.
        /// 无声明默认协议的精确模型身份。
        model_id: String,
    },
    /// A custom provider has no enabled protocol, so no model can bind.
    /// 自定义供应商未启用任何协议，因此任何模型都无法绑定。
    /// The exact model identity that cannot bind.
    /// 无法绑定的精确模型身份。
    NoEnabledProtocols {
        /// The exact model identity that cannot bind.
        /// 无法绑定的精确模型身份。
        model_id: String,
    },
    /// No provider-enabled protocol was observed for this exact model.
    /// 当前模型没有观测到任何供应商已启用协议。
    NoAvailableProtocols {
        /// The exact model identity that cannot bind.
        /// 无法绑定的精确模型身份。
        model_id: String,
    },
    /// An explicit custom preference references a protocol outside the enabled set.
    /// 显式自定义偏好引用了启用集合之外的协议。
    /// The exact model identity and the disabled protocol it still references.
    /// 仍然引用已停用协议的精确模型身份与协议。
    ExplicitProtocolDisabled {
        /// The exact model identity that must be re-selected.
        /// 必须重新选择的精确模型身份。
        model_id: String,
        /// The protocol profile that left the enabled set.
        /// 已离开启用集合的协议 Profile。
        protocol_profile_id: String,
    },
    /// An explicit custom preference names an enabled protocol unavailable for this model.
    /// 显式自定义偏好指定了已启用但当前模型不可用的协议。
    ExplicitProtocolUnavailable {
        /// The exact model identity that must be re-selected.
        /// 必须重新选择的精确模型身份。
        model_id: String,
        /// The enabled protocol absent from this model's observed variants.
        /// 当前模型已观测变体中不存在的已启用协议。
        protocol_profile_id: String,
    },
}

impl std::fmt::Display for ModelProtocolBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownServiceEntry { service_entry_id } => write!(
                formatter,
                "service entry `{service_entry_id}` has no compiled model defaults"
            ),
            Self::Missing { model_id } => write!(
                formatter,
                "model `{model_id}` matches no declared rule and the manifest forbids a default protocol binding"
            ),
            Self::NoSelectedProtocol { model_id, rule_id } => write!(
                formatter,
                "model `{model_id}` matches rule `{rule_id}` which declares no selected protocol"
            ),
            Self::NoDefaultProtocol { model_id } => write!(
                formatter,
                "model `{model_id}` matches no rule and the manifest declares no default protocol"
            ),
            Self::NoEnabledProtocols { model_id } => write!(
                formatter,
                "model `{model_id}` cannot bind because the custom provider declares no enabled protocol"
            ),
            Self::NoAvailableProtocols { model_id } => write!(
                formatter,
                "model `{model_id}` cannot bind because none of its observed protocol variants are enabled"
            ),
            Self::ExplicitProtocolDisabled {
                model_id,
                protocol_profile_id,
            } => write!(
                formatter,
                "model `{model_id}` requires re-selection because protocol `{protocol_profile_id}` is no longer enabled"
            ),
            Self::ExplicitProtocolUnavailable {
                model_id,
                protocol_profile_id,
            } => write!(
                formatter,
                "model `{model_id}` requires re-selection because enabled protocol `{protocol_profile_id}` is unavailable for this model"
            ),
        }
    }
}

/// Resolves the system protocol binding for one model under a compiled manifest.
/// 在一份已编译清单下解析一个模型的系统协议绑定。
///
/// # Parameters
/// 参数
/// `resolved` is the compiled manifest; `service_entry_id` selects the exact entry scope;
/// `credential_profile_id` is the exact credential profile of the connection (`None` when the
/// caller cannot identify one); `model_id` is the exact model identity.
/// `resolved` 是已编译清单；`service_entry_id` 选择精确入口作用域；
/// `credential_profile_id` 是连接所属的精确凭据配置档（调用方无法识别时传 `None`）；
/// `model_id` 是精确模型身份。
///
/// # Returns
/// 返回值
/// A binding with its manifest provenance, or a controlled `ModelProtocolBindingError`.
/// 返回带清单来源的绑定，或受控的 `ModelProtocolBindingError`。
pub fn resolve_system_model_protocol_binding(
    resolved: &ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
) -> Result<ModelProtocolBinding, ModelProtocolBindingError> {
    if let Some(instance) =
        resolved.rule_for_model(service_entry_id, credential_profile_id, model_id)
    {
        let protocol_profile_id = instance.selected_protocol.clone().ok_or_else(|| {
            ModelProtocolBindingError::NoSelectedProtocol {
                model_id: model_id.to_string(),
                rule_id: instance.rule_id.clone(),
            }
        })?;
        return Ok(ModelProtocolBinding {
            protocol_profile_id,
            source: ModelProtocolBindingSource::SystemManifest,
            codex_behavior: instance.codex_behavior.clone(),
            manifest_revision: resolved.revision,
            rule_id: instance.rule_id.clone(),
        });
    }
    match resolved.unknown_model_policy {
        UnknownModelPolicy::DeclaredRuleRequired => Err(ModelProtocolBindingError::Missing {
            model_id: model_id.to_string(),
        }),
        UnknownModelPolicy::DefaultBindingAllowed => {
            let defaults = resolved
                .model_defaults_for_entry(service_entry_id)
                .map_err(|_| ModelProtocolBindingError::UnknownServiceEntry {
                    service_entry_id: service_entry_id.to_string(),
                })?;
            let protocol_profile_id = defaults.selected_protocol.clone().ok_or_else(|| {
                ModelProtocolBindingError::NoDefaultProtocol {
                    model_id: model_id.to_string(),
                }
            })?;
            Ok(ModelProtocolBinding {
                protocol_profile_id,
                source: ModelProtocolBindingSource::SystemManifest,
                codex_behavior: None,
                manifest_revision: resolved.revision,
                rule_id: String::new(),
            })
        }
    }
}

/// Fixed Auto priority across enabled custom-provider protocols (plan §13.3.4).
/// 自定义供应商启用协议间的固定 Auto 优先级（方案 §13.3.4）。
pub const CUSTOM_AUTO_PROTOCOL_PRIORITY: [&str; 5] = [
    OPENAI_RESPONSES_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID,
    ANTHROPIC_MESSAGES_PROFILE_ID,
    OPENAI_CHAT_PROFILE_ID,
    GEMINI_GENERATE_CONTENT_PROFILE_ID,
];

/// Resolves the protocol binding for one custom-provider model.
/// 为一个自定义供应商模型解析协议绑定。
///
/// # Parameters
/// 参数
/// `enabled_protocol_profile_ids` is the exact enabled protocol set of the provider entry;
/// `model_available_protocol_profile_ids` is the exact protocol-variant set observed or explicitly
/// declared for this model; `model_id` identifies the model; `manifest_revision` is the activated
/// custom contract revision; `explicit_protocol_profile_id` is the profile referenced by an
/// explicit user preference (`None` means automatic resolution).
/// `enabled_protocol_profile_ids` 是供应商入口的精确启用协议集合；
/// `model_available_protocol_profile_ids` 是当前模型已观测或显式声明的精确协议变体集合；
/// `model_id` 标识模型；`manifest_revision` 是已激活自定义契约修订；
/// `explicit_protocol_profile_id` 是显式用户偏好引用的 Profile（`None` 表示自动解析）。
///
/// # Returns
/// 返回值
/// A binding that never leaves the enabled protocol set, or a controlled binding error.
/// 返回绝不离开启用协议集合的绑定，或受控的绑定错误。
pub fn resolve_custom_model_protocol_binding(
    enabled_protocol_profile_ids: &[&str],
    model_available_protocol_profile_ids: &[&str],
    model_id: &str,
    manifest_revision: u64,
    explicit_protocol_profile_id: Option<&str>,
) -> Result<ModelProtocolBinding, ModelProtocolBindingError> {
    let enabled: BTreeSet<&str> = enabled_protocol_profile_ids.iter().copied().collect();
    if enabled.is_empty() {
        return Err(ModelProtocolBindingError::NoEnabledProtocols {
            model_id: model_id.to_string(),
        });
    }
    let available: BTreeSet<&str> = model_available_protocol_profile_ids
        .iter()
        .copied()
        .collect();
    if let Some(explicit_protocol) = explicit_protocol_profile_id {
        if !enabled.contains(explicit_protocol) {
            return Err(ModelProtocolBindingError::ExplicitProtocolDisabled {
                model_id: model_id.to_string(),
                protocol_profile_id: explicit_protocol.to_string(),
            });
        }
        if !available.contains(explicit_protocol) {
            return Err(ModelProtocolBindingError::ExplicitProtocolUnavailable {
                model_id: model_id.to_string(),
                protocol_profile_id: explicit_protocol.to_string(),
            });
        }
    }
    // A single enabled protocol produces the unique provider default with no selector.
    // 单一启用协议直接产生唯一供应商默认且不显示选择器。
    if enabled.len() == 1 {
        let only_protocol = *enabled.iter().next().expect("non-empty set");
        if !available.contains(only_protocol) {
            return Err(ModelProtocolBindingError::NoAvailableProtocols {
                model_id: model_id.to_string(),
            });
        }
        return Ok(ModelProtocolBinding {
            protocol_profile_id: only_protocol.to_string(),
            source: ModelProtocolBindingSource::CustomAutoDefault,
            codex_behavior: None,
            manifest_revision,
            rule_id: String::from("unique_protocol"),
        });
    }
    if let Some(explicit_protocol) = explicit_protocol_profile_id {
        return Ok(ModelProtocolBinding {
            protocol_profile_id: explicit_protocol.to_string(),
            source: ModelProtocolBindingSource::CustomUser,
            codex_behavior: None,
            manifest_revision,
            rule_id: String::from("custom_user"),
        });
    }
    let selectable = enabled
        .intersection(&available)
        .copied()
        .collect::<BTreeSet<_>>();
    if selectable.is_empty() {
        return Err(ModelProtocolBindingError::NoAvailableProtocols {
            model_id: model_id.to_string(),
        });
    }
    // Auto resolves by the fixed priority inside the enabled set at the activation boundary;
    // the request and session only read the resolved snapshot and never recompute after errors.
    // Auto 在激活边界按启用集合内的固定优先级解析；请求与会话只读取已解析快照，错误后绝不重算。
    let auto_protocol = CUSTOM_AUTO_PROTOCOL_PRIORITY
        .iter()
        .find(|profile| selectable.contains(**profile))
        .ok_or_else(|| ModelProtocolBindingError::NoAvailableProtocols {
            model_id: model_id.to_string(),
        })?;
    Ok(ModelProtocolBinding {
        protocol_profile_id: (*auto_protocol).to_string(),
        source: ModelProtocolBindingSource::CustomAutoDefault,
        codex_behavior: None,
        manifest_revision,
        rule_id: String::from("custom_auto_default"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_manifest::ManifestOwnership;
    use crate::provider_manifest::compile_manifest_text;

    /// Loads one bundled manifest text by provider id.
    /// 按供应商 ID 加载一份内置清单文本。
    fn bundled_text(provider_id: &str) -> String {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .unwrap_or_else(|_| panic!("missing bundled manifest `{provider_id}`"))
    }

    /// Verifies rule-matched models bind to their declared selected protocol.
    /// 验证命中规则的模型绑定到其声明的生效协议。
    #[test]
    fn rule_match_binds_declared_selected_protocol() {
        let manifest = compile_manifest_text(&bundled_text("deepseek"), ManifestOwnership::System)
            .expect("deepseek manifest must compile");
        let binding = resolve_system_model_protocol_binding(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-pro",
        )
        .expect("v4-pro must bind");
        assert_eq!(binding.protocol_profile_id, "openai-responses.v1");
        assert_eq!(binding.source, ModelProtocolBindingSource::SystemManifest);
        assert_eq!(binding.rule_id, "v4-pro");
        assert_eq!(binding.manifest_revision, manifest.revision);

        let flash = resolve_system_model_protocol_binding(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-flash",
        )
        .expect("deepseek flash must bind");
        assert_eq!(flash.protocol_profile_id, "openai-responses.v1");
        assert_eq!(binding.protocol_profile_id, flash.protocol_profile_id);
    }

    /// Verifies MiniMax M3 uses its explicitly selected video-capable chat protocol.
    /// 验证 MiniMax M3 使用显式选择的支持视频对话协议。
    #[test]
    fn minimax_m3_binding_is_fixed_to_video_capable_chat_profile() {
        // manifest is the production MiniMax routing authority for the exact coding-plan entry.
        // manifest 是精确 coding-plan 入口的生产 MiniMax 路由权威。
        let manifest = compile_manifest_text(&bundled_text("minimax"), ManifestOwnership::System)
            .expect("MiniMax manifest must compile");
        let binding = resolve_system_model_protocol_binding(
            &manifest,
            "cn",
            Some("token-plan-key"),
            "MiniMax-M3",
        )
        .expect("MiniMax M3 must have a frozen protocol binding");
        assert_eq!(binding.protocol_profile_id, OPENAI_CHAT_PROFILE_ID);
        assert_eq!(binding.source, ModelProtocolBindingSource::SystemManifest);
        assert_eq!(binding.rule_id, "reasoning-minimax-m3");
        assert_eq!(binding.manifest_revision, manifest.revision);
    }

    /// Verifies a credential-scoped rule outranks a global rule with the same matcher.
    /// 验证相同匹配器下凭据作用域规则优先于全局规则。
    #[test]
    fn credential_scoped_rule_outranks_global_rule() {
        let text = r#"{
            "schema_version": "vulcan.provider-manifest.v2",
            "provider_id": "probe-provider",
            "revision": 1,
            "published_at": "2026-08-07T00:00:00Z",
            "display": { "name": "Probe Provider", "category": "official" },
            "engine_contract": {
                "minimum_manifest_engine": 1,
                "required_protocol_profiles": ["openai-chat-completions.v1", "openai-responses.v1"]
            },
            "services": [
                {
                    "service_entry_id": "official-api",
                    "display_name": "Official API",
                    "kind": "conversation",
                    "base_url": "https://api.probe.example.com",
                    "credential_profiles": [
                        { "credential_profile_id": "api-key", "display_name": "API Key", "credential_mode": "token" },
                        { "credential_profile_id": "admin-key", "display_name": "Admin Key", "credential_mode": "token" }
                    ],
                    "default_authentication": "bearer_authorization",
                    "model_catalog": { "source_policy": "manual_only", "discovery_kind": "manual" },
                    "protocol_routes": [
                        { "protocol_profile_id": "openai-chat-completions.v1", "endpoint_path": "/v1/chat/completions", "verified": true },
                        { "protocol_profile_id": "openai-responses.v1", "endpoint_path": "/v1/responses", "verified": true }
                    ]
                }
            ],
            "model_defaults": {
                "default_enabled": true,
                "catalog_membership_policy": "declared_only",
                "unknown_model_policy": "declared_rule_required"
            },
            "model_rules": [
                {
                    "rule_id": "global",
                    "matcher": { "kind": "exact", "value": "m1" },
                    "selected_protocol": "openai-chat-completions.v1"
                },
                {
                    "rule_id": "scoped",
                    "scope": { "credential_profile_ids": ["api-key"] },
                    "matcher": { "kind": "exact", "value": "m1" },
                    "selected_protocol": "openai-responses.v1"
                }
            ],
            "provenance": { "status": "migrated", "sources": [] }
        }"#;
        let manifest =
            compile_manifest_text(text, ManifestOwnership::System).expect("probe must compile");
        let binding =
            resolve_system_model_protocol_binding(&manifest, "official-api", Some("api-key"), "m1")
                .expect("m1 under the exact credential profile must bind");
        assert_eq!(binding.rule_id, "scoped");
        assert_eq!(binding.protocol_profile_id, "openai-responses.v1");

        // A connection whose profile cannot be identified only sees the global rule.
        // 无法识别配置档的连接只命中全局规则。
        let binding = resolve_system_model_protocol_binding(&manifest, "official-api", None, "m1")
            .expect("m1 without a profile must bind through the global rule");
        assert_eq!(binding.rule_id, "global");
        assert_eq!(binding.protocol_profile_id, "openai-chat-completions.v1");
    }

    /// Verifies a newly discovered OpenRouter model may use the reviewed default route.
    /// 验证新发现的 OpenRouter 模型可使用已审核的默认路由。
    #[test]
    fn openrouter_discovery_uses_default_binding_after_account_selection() {
        let manifest =
            compile_manifest_text(&bundled_text("openrouter"), ManifestOwnership::System)
                .expect("openrouter manifest must compile");
        let binding = resolve_system_model_protocol_binding(
            &manifest,
            "public-api",
            Some("api-key"),
            "openai/gpt-4o",
        )
        .expect("discovered model may bind after account selection");
        assert_eq!(binding.protocol_profile_id, "anthropic-messages.v1");
        assert!(binding.rule_id.is_empty());
    }

    /// Verifies DeepSeek rejects identities outside its fixed manifest inventory.
    /// 验证 DeepSeek 拒绝固定 Manifest 清单之外的模型身份。
    #[test]
    fn deepseek_fixed_inventory_rejects_unknown_models() {
        // manifest is the production DeepSeek identity and routing authority.
        // manifest 是生产 DeepSeek 模型身份与路由权威。
        let manifest = compile_manifest_text(&bundled_text("deepseek"), ManifestOwnership::System)
            .expect("deepseek manifest must compile");
        // error proves an undeclared identity cannot acquire a default binding.
        // error 证明未声明身份无法获得默认绑定。
        let error = resolve_system_model_protocol_binding(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-r2",
        )
        .expect_err("unknown DeepSeek model must not bind");
        assert!(matches!(error, ModelProtocolBindingError::Missing { .. }));
    }

    /// Verifies entry-level defaults prevent subscription routes from inheriting another entry's protocol.
    /// 验证入口级默认值可防止订阅路由继承其他入口的协议。
    #[test]
    fn default_binding_uses_exact_service_entry_override() {
        let manifest = compile_manifest_text(&bundled_text("anthropic"), ManifestOwnership::System)
            .expect("anthropic manifest must compile");
        let public_binding = resolve_system_model_protocol_binding(
            &manifest,
            "public-api",
            Some("api-key"),
            "claude-future-public",
        )
        .expect("public unknown model must use the public entry default");
        assert_eq!(
            public_binding.protocol_profile_id,
            ANTHROPIC_MESSAGES_PROFILE_ID
        );

        let subscription_binding = resolve_system_model_protocol_binding(
            &manifest,
            "claude-code-subscription",
            Some("claude-code-oauth"),
            "claude-future-subscription",
        )
        .expect("subscription unknown model must use the subscription entry default");
        assert_eq!(
            subscription_binding.protocol_profile_id,
            "claude-code-messages.v1"
        );
    }

    /// Verifies Auto resolves by the fixed priority inside the enabled set (plan §13.3.4).
    /// 验证 Auto 在启用集合内按固定优先级解析（方案 §13.3.4）。
    #[test]
    fn custom_auto_follows_fixed_protocol_priority() {
        let binding = resolve_custom_model_protocol_binding(
            &["anthropic-messages.v1", "openai-chat-completions.v1"],
            &["anthropic-messages.v1", "openai-chat-completions.v1"],
            "custom-model-1",
            17,
            None,
        )
        .expect("auto must bind to the highest priority enabled protocol");
        assert_eq!(binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID);
        assert_eq!(
            binding.source,
            ModelProtocolBindingSource::CustomAutoDefault
        );
        assert_eq!(binding.rule_id, "custom_auto_default");
        assert_eq!(binding.manifest_revision, 17);

        let binding = resolve_custom_model_protocol_binding(
            &[OPENAI_CHAT_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID],
            &[OPENAI_CHAT_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID],
            "custom-model-2",
            17,
            None,
        )
        .expect("auto must prefer chat over gemini");
        assert_eq!(binding.protocol_profile_id, OPENAI_CHAT_PROFILE_ID);

        let binding = resolve_custom_model_protocol_binding(
            &[
                "openai-chat-completions.v1",
                "openai-responses.v1",
                "anthropic-messages.v1",
            ],
            &[
                "openai-chat-completions.v1",
                "openai-responses.v1",
                "anthropic-messages.v1",
            ],
            "custom-model-3",
            17,
            None,
        )
        .expect("auto must prefer responses first");
        assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);
    }

    /// Verifies one enabled protocol produces the unique binding without a selector.
    /// 验证单一启用协议产生唯一绑定且无选择器。
    #[test]
    fn custom_single_protocol_produces_unique_binding() {
        let binding = resolve_custom_model_protocol_binding(
            &[GEMINI_GENERATE_CONTENT_PROFILE_ID],
            &[GEMINI_GENERATE_CONTENT_PROFILE_ID],
            "custom-model-4",
            23,
            None,
        )
        .expect("single protocol must bind");
        assert_eq!(
            binding.protocol_profile_id,
            GEMINI_GENERATE_CONTENT_PROFILE_ID
        );
        assert_eq!(
            binding.source,
            ModelProtocolBindingSource::CustomAutoDefault
        );
        assert_eq!(binding.rule_id, "unique_protocol");
        assert_eq!(binding.manifest_revision, 23);
    }

    /// Verifies explicit preferences bind inside the enabled set and fail outside it.
    /// 验证显式偏好仅在启用集合内绑定，集合外必须失败。
    #[test]
    fn custom_explicit_binds_inside_and_requires_reselection_outside() {
        let binding = resolve_custom_model_protocol_binding(
            &["openai-responses.v1", "anthropic-messages.v1"],
            &["openai-responses.v1", "anthropic-messages.v1"],
            "custom-model-5",
            29,
            Some("anthropic-messages.v1"),
        )
        .expect("explicit protocol inside the enabled set must bind");
        assert_eq!(binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID);
        assert_eq!(binding.source, ModelProtocolBindingSource::CustomUser);
        assert_eq!(binding.rule_id, "custom_user");
        assert_eq!(binding.manifest_revision, 29);

        let error = resolve_custom_model_protocol_binding(
            &["openai-responses.v1"],
            &["openai-responses.v1"],
            "custom-model-6",
            29,
            Some("openai-chat-completions.v1"),
        )
        .expect_err("explicit protocol outside the enabled set must be rejected");
        assert!(matches!(
            error,
            ModelProtocolBindingError::ExplicitProtocolDisabled { .. }
        ));
    }

    /// Verifies an empty enabled set never binds any model.
    /// 验证空启用集合绝不绑定任何模型。
    #[test]
    fn custom_empty_enabled_set_never_binds() {
        let error = resolve_custom_model_protocol_binding(&[], &[], "custom-model-7", 31, None)
            .expect_err("empty enabled set must be rejected");
        assert!(matches!(
            error,
            ModelProtocolBindingError::NoEnabledProtocols { .. }
        ));
    }

    /// Verifies Auto and Explicit choices are constrained by this model's observed variants.
    /// 验证 Auto 与显式选择均受当前模型已观测协议变体约束。
    #[test]
    fn custom_binding_uses_model_available_protocol_intersection() {
        let binding = resolve_custom_model_protocol_binding(
            &[OPENAI_RESPONSES_PROFILE_ID, ANTHROPIC_MESSAGES_PROFILE_ID],
            &[ANTHROPIC_MESSAGES_PROFILE_ID],
            "anthropic-only-model",
            37,
            None,
        )
        .expect("auto must select the highest-priority protocol available for this model");
        assert_eq!(binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID);

        let error = resolve_custom_model_protocol_binding(
            &[OPENAI_RESPONSES_PROFILE_ID, ANTHROPIC_MESSAGES_PROFILE_ID],
            &[ANTHROPIC_MESSAGES_PROFILE_ID],
            "anthropic-only-model",
            37,
            Some(OPENAI_RESPONSES_PROFILE_ID),
        )
        .expect_err("an enabled but model-unavailable explicit protocol must fail");
        assert!(matches!(
            error,
            ModelProtocolBindingError::ExplicitProtocolUnavailable { .. }
        ));
    }
}
