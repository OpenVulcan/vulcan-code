//! Model-rule inheritance: provider defaults as the base, entry overlays and rules as overrides.
//! 模型规则继承：供应商默认为基座，入口覆盖与规则作为覆盖。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vulcan_ai_contract::{ModelCapabilities, ModelParameterDefinition};

use super::{
    FieldSourceKind, ManifestCapabilityPatch, ProviderManifestV1, ResolvedRuleProvenance,
    RuleMatcher,
};

/// One resolved capability field's provenance.
/// 一个已解析能力字段的来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedFieldProvenance {
    /// Which manifest layer produced the value.
    /// 值由哪一层清单产生。
    pub source: FieldSourceKind,
    /// Rule identifier when the value came from a model rule; `None` for provider defaults.
    /// 值来自模型规则时的规则标识；来自供应商默认时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Manifest revision that produced the value.
    /// 产生值的清单修订号。
    pub manifest_revision: u64,
    /// Evidence revision declared by the producing layer when present.
    /// 产生层声明的证据修订号（存在时）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_revision: Option<String>,
    /// Ordered lower-precedence sources deterministically replaced by the winner.
    /// 被最终胜者确定性覆盖的有序低优先级来源。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overridden_sources: Vec<ResolvedFieldOrigin>,
}

/// One immutable field origin retained after a higher-precedence source wins.
/// 一个在更高优先级来源胜出后保留的不可变字段来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedFieldOrigin {
    /// Manifest layer that produced the overridden value.
    /// 产生被覆盖值的 Manifest 层。
    pub source: FieldSourceKind,
    /// Optional exact rule identifier.
    /// 可选精确规则标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Manifest revision that produced the overridden value.
    /// 产生被覆盖值的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Optional evidence revision attached to the overridden value.
    /// 附加到被覆盖值的可选证据修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_revision: Option<String>,
}

impl ResolvedFieldProvenance {
    /// Converts the current winner into an immutable overridden-origin record.
    /// 把当前胜者转换为不可变的被覆盖来源记录。
    fn winner_origin(&self) -> ResolvedFieldOrigin {
        ResolvedFieldOrigin {
            source: self.source,
            rule_id: self.rule_id.clone(),
            manifest_revision: self.manifest_revision,
            evidence_revision: self.evidence_revision.clone(),
        }
    }
}

/// Attaches deterministic lower-precedence winners to one final field provenance record.
/// 把确定性的低优先级胜者附加到一个最终字段来源记录。
fn provenance_with_overridden(
    mut winner: ResolvedFieldProvenance,
    overridden: Vec<ResolvedFieldProvenance>,
) -> ResolvedFieldProvenance {
    winner.overridden_sources = overridden
        .iter()
        .map(ResolvedFieldProvenance::winner_origin)
        .collect();
    winner
}

/// Applies one manifest capability patch to a target with Inherit/Clear/Set semantics.
/// 以 Inherit/Clear/Set 语义把一个 Manifest 能力补丁应用到目标。
///
/// An omitted patch field leaves the target untouched (Inherit); `null` on a nullable field
/// clears it (Clear); a concrete value replaces it (Set). The `written` slice receives every
/// field name actually written so callers can build per-field provenance (plan §6.4.6).
/// 补丁中省略的字段不触碰目标（Inherit）；可空字段的 `null` 清除它（Clear）；
/// 具体值替换它（Set）。`written` 收集每个实际被写入的字段名，供调用方构建
/// 逐字段来源（方案 §6.4.6）。
pub fn apply_capability_patch(
    target: &mut ModelCapabilities,
    patch: &ManifestCapabilityPatch,
    written: &mut Vec<&'static str>,
) {
    if let Some(operations) = &patch.operations {
        target.operations = operations.clone();
        written.push("operations");
    }
    if let Some(input_resources) = &patch.input_resources {
        target.input_resources = input_resources.clone();
        written.push("input_resources");
    }
    if let Some(output_resources) = &patch.output_resources {
        target.output_resources = output_resources.clone();
        written.push("output_resources");
    }
    if let Some(embedding) = &patch.embedding {
        target.embedding = embedding.clone();
        written.push("embedding");
    }
    if let Some(rerank) = &patch.rerank {
        target.rerank = rerank.clone();
        written.push("rerank");
    }
    if let Some(reasoning) = patch.reasoning {
        target.reasoning = reasoning;
        written.push("reasoning");
    }
    if let Some(provider_native_tools) = &patch.provider_native_tools {
        target.provider_native_tools = provider_native_tools.clone();
        written.push("provider_native_tools");
    }
    if let Some(reasoning_control) = patch.reasoning_control {
        target.reasoning_control = reasoning_control;
        written.push("reasoning_control");
    }
    if let Some(reasoning_efforts) = &patch.reasoning_efforts {
        target.reasoning_efforts = reasoning_efforts.clone();
        written.push("reasoning_efforts");
    }
    if let Some(context_limit) = patch.context_limit {
        target.context_limit = context_limit;
        written.push("context_limit");
    }
    if let Some(output_limit) = patch.output_limit {
        target.output_limit = output_limit;
        written.push("output_limit");
    }
    if let Some(prompt_cache) = &patch.prompt_cache {
        target.prompt_cache = prompt_cache.clone();
        written.push("prompt_cache");
    }
    if let Some(model_parameters) = &patch.model_parameters {
        // Parameters merge by stable ID when present; an explicit empty collection is the
        // closed Clear state and removes inherited parameters (plan §6.4 presence semantics).
        // 参数存在时按稳定 ID 合并；显式空集合是封闭 Clear 状态并移除继承参数
        // （方案 §6.4 presence 语义）。
        if model_parameters.is_empty() {
            target.model_parameters.clear();
        } else {
            for parameter in model_parameters {
                if let Some(existing) = target
                    .model_parameters
                    .iter_mut()
                    .find(|existing| existing.id == parameter.id)
                {
                    *existing = parameter.clone();
                } else {
                    target.model_parameters.push(parameter.clone());
                }
            }
        }
        written.push("model_parameters");
    }
    if let Some(evidence_revision) = &patch.evidence_revision {
        target.evidence_revision = evidence_revision.clone();
        written.push("evidence_revision");
    }
}

/// One rule expanded for one service entry with fully inherited effective fields.
/// 一条规则按服务入口展开、携带完整继承有效字段的实例。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRuleInstance {
    /// Stable rule identifier.
    /// 稳定规则标识。
    pub rule_id: String,
    /// Exact or longest-prefix provenance classification.
    /// 精确或最长前缀来源分类。
    pub source: FieldSourceKind,
    /// Rule matcher shared by every entry instance.
    /// 所有入口实例共享的规则匹配器。
    pub matcher: RuleMatcher,
    /// Rule-owned user-visible model name.
    /// 规则持有的用户可见模型名称。
    pub display_name: Option<String>,
    /// Service entry this instance applies to.
    /// 此实例作用的服务入口。
    pub service_entry_id: String,
    /// Credential-profile scope restriction; empty means all profiles.
    /// 凭据配置档作用域限制；空表示全部配置档。
    pub credential_profile_ids: Vec<String>,
    /// Concrete normalized target set shared by every instance of the same rule; the
    /// deterministic scope ordering used by validation and compilation (plan §6.5).
    /// 同一条规则所有实例共享的具体归一化目标集合；验证与编译共用的确定性
    /// 作用域排序依据（方案 §6.5）。
    pub normalized_scope: super::scope::NormalizedRuleTargets,
    /// Inherited supported protocol candidate set.
    /// 继承后的支持协议候选集合。
    pub supported_protocols: Vec<String>,
    /// Inherited selected protocol; may be `None` when no layer declares one.
    /// 继承后的生效协议；无任何层声明时可为 `None`。
    pub selected_protocol: Option<String>,
    /// Inherited protocol-independent capability template.
    /// 继承后的协议无关能力模板。
    pub capabilities: Option<ModelCapabilities>,
    /// Per-field provenance of the resolved capability template keyed by field name;
    /// protocol-scoped fields use the `protocol.<protocol_id>.<field>` key form.
    /// 已解析能力模板的逐字段来源，按键为字段名；协议作用域字段使用
    /// `protocol.<protocol_id>.<field>` 键形式。
    pub capability_provenance: BTreeMap<String, ResolvedFieldProvenance>,
    /// Per-field provenance for non-capability inherited routing and profile fields.
    /// 非能力继承路由与 Profile 字段的逐字段来源。
    pub field_provenance: BTreeMap<String, ResolvedFieldProvenance>,
    /// Protocol-scoped capability templates.
    /// 协议作用域能力模板。
    pub protocol_capabilities: BTreeMap<String, ModelCapabilities>,
    /// Model-specific resource constraints merged into the frozen selected route.
    /// 合并到冻结所选路由的模型专属资源约束。
    pub input_resource_constraints: Vec<crate::InputResourceConstraint>,
    /// Inherited default-enabled state.
    /// 继承后的默认启用状态。
    pub default_enabled: bool,
    /// Inherited usage-extraction profile references.
    /// 继承后的 usage 提取 Profile 引用。
    pub usage_profile_refs: Vec<String>,
    /// Inherited request-projection profile references.
    /// 继承后的请求投影 Profile 引用。
    pub request_profile_refs: Vec<String>,
    /// Inherited response/error profile references.
    /// 继承后的响应/错误 Profile 引用。
    pub response_profile_refs: Vec<String>,
    /// Inherited operation template references.
    /// 继承后的操作模板引用。
    pub operation_refs: Vec<String>,
    /// Rule-owned account-plan allowlist; empty means no plan-based filtering.
    /// 规则持有的账号套餐白名单；为空表示不按套餐过滤。
    pub allowed_account_plan_names: Vec<String>,
    /// Inherited model-specific parameter definitions.
    /// 继承后的模型独立参数定义。
    pub model_parameters: Vec<ModelParameterDefinition>,
    /// Rule provenance frozen at compile time.
    /// 编译时冻结的规则来源。
    pub provenance: ResolvedRuleProvenance,
}

/// Expands every model rule into per-service-entry instances with inherited effective fields.
/// 将每条模型规则展开为携带继承有效字段的按服务入口实例。
///
/// Provider defaults form the inheritance base; entry overlays refine protocols; rules override
/// the base for the entries they cover. Rules with an empty scope expand to every service entry.
/// 供应商默认构成继承基座；入口覆盖细化协议；规则对其覆盖的入口覆盖基座。
/// 无作用域的规则展开到全部服务入口。
pub fn resolve_rule_instances(manifest: &ProviderManifestV1) -> Vec<ResolvedRuleInstance> {
    let mut instances = Vec::new();
    for rule in manifest.model_rules.iter() {
        // The normalized target set is a rule-level fact; every expanded instance of the
        // rule carries the same set so ordering never depends on JSON array position.
        // 归一化目标集合是规则级事实；规则展开出的每个实例携带同一集合，
        // 使排序永不依赖 JSON 数组位置。
        let normalized_scope = super::scope::NormalizedRuleTargets::from_rule(manifest, rule);
        let targets: Vec<&crate::provider_manifest::schema::ManifestServiceEntry> = manifest
            .services
            .iter()
            .filter(|entry| rule.scope.covers_service_entry(&entry.service_entry_id))
            .collect();
        for entry in targets {
            let mut instance = resolve_rule_for_entry(manifest, rule, entry);
            instance.normalized_scope = normalized_scope.clone();
            instances.push(instance);
        }
    }
    instances
}

/// Resolves one rule against one service entry.
/// 针对单个服务入口解析一条规则。
fn resolve_rule_for_entry(
    manifest: &ProviderManifestV1,
    rule: &crate::provider_manifest::schema::ModelRule,
    entry: &crate::provider_manifest::schema::ManifestServiceEntry,
) -> ResolvedRuleInstance {
    let defaults = &manifest.model_defaults;
    let overlay = entry.model_defaults_override.as_ref();

    let supported_protocols = if !rule.supported_protocols.is_empty() {
        rule.supported_protocols.clone()
    } else if let Some(protocols) = overlay.and_then(|o| o.supported_protocols.clone()) {
        protocols
    } else {
        defaults.supported_protocols.clone()
    };

    let selected_protocol = rule
        .selected_protocol
        .clone()
        .or_else(|| overlay.and_then(|o| o.selected_protocol.clone()))
        .or_else(|| defaults.selected_protocol.clone());

    // Exact and longest-prefix provenance classification shared by every capability field
    // written by this rule.
    // 该规则写入的每个能力字段共享的精确或最长前缀来源分类。
    let source = match rule.matcher.kind {
        super::MatcherKind::Exact => FieldSourceKind::ModelRuleExact,
        super::MatcherKind::Prefix => FieldSourceKind::ModelRulePrefix,
    };

    // Capability patches merge field-by-field with real presence semantics instead of
    // replacing the whole object (plan §6.4): the provider default forms the base, the rule
    // overlays only the fields it declares, and each written field records its provenance.
    // 能力补丁按真实 presence 语义逐字段合并而不是整对象替换（方案 §6.4）：
    // 供应商默认构成基座，规则只覆盖其声明的字段，且每个被写入字段记录来源。
    let default_source = FieldSourceKind::ProviderDefaults;
    let (capabilities, capability_provenance) =
        match (rule.capabilities.as_ref(), defaults.capabilities.as_ref()) {
            (Some(rule_caps), Some(default_caps)) => {
                let mut merged = ModelCapabilities::default();
                let mut provenance = BTreeMap::new();
                let mut written = Vec::new();
                apply_capability_patch(&mut merged, default_caps, &mut written);
                for field in written {
                    provenance.insert(
                        field.to_string(),
                        ResolvedFieldProvenance {
                            source: default_source,
                            rule_id: None,
                            manifest_revision: manifest.revision,
                            evidence_revision: default_caps.evidence_revision.clone(),
                            overridden_sources: Vec::new(),
                        },
                    );
                }
                let mut written = Vec::new();
                apply_capability_patch(&mut merged, rule_caps, &mut written);
                for field in written {
                    let overridden = provenance
                        .get(field)
                        .cloned()
                        .into_iter()
                        .collect::<Vec<_>>();
                    provenance.insert(
                        field.to_string(),
                        provenance_with_overridden(
                            ResolvedFieldProvenance {
                                source,
                                rule_id: Some(rule.rule_id.clone()),
                                manifest_revision: manifest.revision,
                                evidence_revision: rule_caps.evidence_revision.clone(),
                                overridden_sources: Vec::new(),
                            },
                            overridden,
                        ),
                    );
                }
                (Some(merged), provenance)
            }
            (Some(rule_caps), None) => {
                let mut merged = ModelCapabilities::default();
                let mut provenance = BTreeMap::new();
                let mut written = Vec::new();
                apply_capability_patch(&mut merged, rule_caps, &mut written);
                for field in written {
                    provenance.insert(
                        field.to_string(),
                        ResolvedFieldProvenance {
                            source,
                            rule_id: Some(rule.rule_id.clone()),
                            manifest_revision: manifest.revision,
                            evidence_revision: rule_caps.evidence_revision.clone(),
                            overridden_sources: Vec::new(),
                        },
                    );
                }
                (Some(merged), provenance)
            }
            (None, default_caps) => match default_caps {
                Some(default_caps) => {
                    let mut merged = ModelCapabilities::default();
                    let mut provenance = BTreeMap::new();
                    let mut written = Vec::new();
                    apply_capability_patch(&mut merged, default_caps, &mut written);
                    for field in written {
                        provenance.insert(
                            field.to_string(),
                            ResolvedFieldProvenance {
                                source: default_source,
                                rule_id: None,
                                manifest_revision: manifest.revision,
                                evidence_revision: default_caps.evidence_revision.clone(),
                                overridden_sources: Vec::new(),
                            },
                        );
                    }
                    (Some(merged), provenance)
                }
                None => (None, BTreeMap::new()),
            },
        };

    // Protocol-scoped patches compile into concrete capability snapshots from the same
    // conservative base; every field written by a protocol patch records its provenance.
    // 协议作用域补丁从同一保守基座编译为具体能力快照；协议补丁写入的每个字段记录来源。
    let mut protocol_capabilities = BTreeMap::new();
    let mut capability_provenance = capability_provenance;
    for (protocol, patch) in &rule.protocol_capabilities {
        let mut compiled = ModelCapabilities::default();
        let mut written = Vec::new();
        apply_capability_patch(&mut compiled, patch, &mut written);
        for field in written {
            capability_provenance.insert(
                format!("protocol.{protocol}.{field}"),
                ResolvedFieldProvenance {
                    source,
                    rule_id: Some(rule.rule_id.clone()),
                    manifest_revision: manifest.revision,
                    evidence_revision: patch.evidence_revision.clone(),
                    overridden_sources: Vec::new(),
                },
            );
        }
        protocol_capabilities.insert(protocol.clone(), compiled);
    }

    // Reference collections use true presence semantics: omission inherits, a non-empty list
    // replaces, and an explicit empty list clears the inherited collection (plan §6.4/§15.4).
    // 引用集合使用真实 presence 语义：省略即继承，非空列表替换，显式空列表清除继承集合
    // （方案 §6.4/§15.4）。
    let usage_profile_refs = rule
        .usage_profile_refs
        .clone()
        .unwrap_or_else(|| defaults.default_usage_profile_refs.clone());
    let request_profile_refs = rule
        .request_profile_refs
        .clone()
        .unwrap_or_else(|| defaults.default_request_profile_refs.clone());
    let response_profile_refs = rule
        .response_profile_refs
        .clone()
        .unwrap_or_else(|| defaults.default_response_profile_refs.clone());
    let operation_refs = rule
        .operation_refs
        .clone()
        .unwrap_or_else(|| defaults.default_operation_refs.clone());

    // field_provenance records only the final winner for every resolved non-capability field;
    // it never retains ambiguous candidate chains.
    // field_provenance 只记录每个非能力解析字段的最终胜者；绝不保留模糊候选链。
    let mut field_provenance = BTreeMap::new();
    let provider_provenance = || ResolvedFieldProvenance {
        source: FieldSourceKind::ProviderDefaults,
        rule_id: None,
        manifest_revision: manifest.revision,
        evidence_revision: None,
        overridden_sources: Vec::new(),
    };
    let entry_provenance = || ResolvedFieldProvenance {
        source: FieldSourceKind::ServiceEntryOverride,
        rule_id: None,
        manifest_revision: manifest.revision,
        evidence_revision: None,
        overridden_sources: Vec::new(),
    };
    let rule_provenance = || ResolvedFieldProvenance {
        source,
        rule_id: Some(rule.rule_id.clone()),
        manifest_revision: manifest.revision,
        evidence_revision: None,
        overridden_sources: Vec::new(),
    };
    let entry_declares_supported = overlay
        .and_then(|value| value.supported_protocols.as_ref())
        .is_some();
    let supported_provenance = if !rule.supported_protocols.is_empty() {
        let mut overridden = vec![provider_provenance()];
        if entry_declares_supported {
            overridden.push(entry_provenance());
        }
        provenance_with_overridden(rule_provenance(), overridden)
    } else if entry_declares_supported {
        provenance_with_overridden(entry_provenance(), vec![provider_provenance()])
    } else {
        provider_provenance()
    };
    field_provenance.insert(String::from("supported_protocols"), supported_provenance);
    if selected_protocol.is_some() {
        let entry_declares_selected = overlay
            .and_then(|value| value.selected_protocol.as_ref())
            .is_some();
        let selected_provenance = if rule.selected_protocol.is_some() {
            let mut overridden = Vec::new();
            if defaults.selected_protocol.is_some() {
                overridden.push(provider_provenance());
            }
            if entry_declares_selected {
                overridden.push(entry_provenance());
            }
            provenance_with_overridden(rule_provenance(), overridden)
        } else if entry_declares_selected {
            let overridden = defaults
                .selected_protocol
                .as_ref()
                .map(|_| provider_provenance())
                .into_iter()
                .collect();
            provenance_with_overridden(entry_provenance(), overridden)
        } else {
            provider_provenance()
        };
        field_provenance.insert(String::from("selected_protocol"), selected_provenance);
    }
    field_provenance.insert(
        String::from("default_enabled"),
        if rule.default_enabled.is_some() {
            provenance_with_overridden(rule_provenance(), vec![provider_provenance()])
        } else {
            provider_provenance()
        },
    );
    for (field, overridden) in [
        ("usage_profile_refs", rule.usage_profile_refs.is_some()),
        ("request_profile_refs", rule.request_profile_refs.is_some()),
        (
            "response_profile_refs",
            rule.response_profile_refs.is_some(),
        ),
        ("operation_refs", rule.operation_refs.is_some()),
    ] {
        field_provenance.insert(
            field.to_string(),
            if overridden {
                provenance_with_overridden(rule_provenance(), vec![provider_provenance()])
            } else {
                provider_provenance()
            },
        );
    }
    if !rule.model_parameters.is_empty() {
        field_provenance.insert(String::from("model_parameters"), rule_provenance());
    }
    if !rule.input_resource_constraints.is_empty() {
        field_provenance.insert(
            String::from("input_resource_constraints"),
            rule_provenance(),
        );
    }
    if !rule.allowed_account_plan_names.is_empty() {
        field_provenance.insert(
            String::from("allowed_account_plan_names"),
            rule_provenance(),
        );
    }
    if rule.display_name.is_some() {
        field_provenance.insert(String::from("display_name"), rule_provenance());
    }

    ResolvedRuleInstance {
        rule_id: rule.rule_id.clone(),
        source,
        matcher: rule.matcher.clone(),
        display_name: rule.display_name.clone(),
        service_entry_id: entry.service_entry_id.clone(),
        credential_profile_ids: rule.scope.credential_profile_ids.clone(),
        normalized_scope: super::scope::NormalizedRuleTargets::default(),
        supported_protocols,
        selected_protocol,
        capabilities,
        capability_provenance,
        field_provenance,
        protocol_capabilities,
        input_resource_constraints: rule.input_resource_constraints.clone(),
        default_enabled: rule.default_enabled.unwrap_or(defaults.default_enabled),
        usage_profile_refs,
        request_profile_refs,
        response_profile_refs,
        operation_refs,
        allowed_account_plan_names: rule.allowed_account_plan_names.clone(),
        model_parameters: rule.model_parameters.clone(),
        provenance: ResolvedRuleProvenance {
            source,
            rule_id: rule.rule_id.clone(),
            manifest_revision: manifest.revision,
            provenance_ref: rule.provenance_ref.clone(),
            provenance_status: manifest.provenance.status,
        },
    }
}
