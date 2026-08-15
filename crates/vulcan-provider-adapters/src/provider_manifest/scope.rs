//! Rule-scope normalization and deterministic ordering shared by validation and compilation.
//! 规则作用域归一化与确定性排序，验证与编译共享同一实现（方案 §6.5）。

use std::collections::BTreeSet;

use super::{ModelRule, ProviderManifestV1};

/// Concrete target set of one rule scope: every covered `(service_entry_id, credential_profile_id)` pair.
/// 一条规则作用域的具体目标集合：每个被覆盖的 `(服务入口标识, 凭据配置档标识)` 组合。
///
/// Empty `service_entry_ids` covers every declared entry; empty `credential_profile_ids` covers
/// every credential profile of each covered entry. The expansion is performed at compile time so
/// JSON rule array order can never be a tie-breaker (plan §6.5.1/§6.5.7).
/// `service_entry_ids` 为空表示覆盖全部声明入口；`credential_profile_ids` 为空表示覆盖每个
/// 被覆盖入口的全部凭据配置档。展开在编译期完成，因此 JSON 规则数组顺序绝不充当决胜依据
/// （方案 §6.5.1/§6.5.7）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NormalizedRuleTargets {
    /// Sorted unique target pairs.
    /// 排序去重后的目标组合。
    targets: BTreeSet<(String, String)>,
}

impl NormalizedRuleTargets {
    /// Expands one rule scope against the manifest trust domain.
    /// 针对清单信任域展开一条规则作用域。
    ///
    /// # Parameters
    /// 参数
    /// `manifest` owns the declared service entries and `rule` is the exact model rule.
    /// `manifest` 持有声明的服务入口，`rule` 是精确模型规则。
    ///
    /// # Returns
    /// 返回值
    /// The concrete target set. Scope identifiers referencing entries or profiles that do not
    /// exist are dropped here; validation reports them separately with exact error paths.
    /// 返回具体目标集合。引用不存在入口或配置档的作用域标识在此处被丢弃；
    /// 验证器会以精确错误路径单独报告它们。
    pub fn from_rule(manifest: &ProviderManifestV1, rule: &ModelRule) -> Self {
        let mut targets = BTreeSet::new();
        for entry in manifest.services.iter() {
            if !rule.scope.covers_service_entry(&entry.service_entry_id) {
                continue;
            }
            if rule.scope.credential_profile_ids.is_empty() {
                for profile in &entry.credential_profiles {
                    targets.insert((
                        entry.service_entry_id.clone(),
                        profile.credential_profile_id.clone(),
                    ));
                }
            } else {
                for profile_id in &rule.scope.credential_profile_ids {
                    targets.insert((entry.service_entry_id.clone(), profile_id.clone()));
                }
            }
        }
        Self { targets }
    }

    /// Reports the deterministic relation between this target set and another.
    /// 报告该目标集合与另一目标集合之间的确定性关系。
    ///
    /// # Parameters
    /// 参数
    /// `other` is the second normalized target set under the same matcher rank.
    /// `other` 是同一匹配器排名下的第二个归一化目标集合。
    ///
    /// # Returns
    /// 返回值
    /// One of `Identical`, `StrictSubset`, `StrictSuperset`, `Disjoint`, or `Incomparable`.
    /// 返回 `Identical`、`StrictSubset`、`StrictSuperset`、`Disjoint` 或 `Incomparable` 之一。
    pub fn relation(&self, other: &Self) -> ScopeRelation {
        let self_is_subset = self.targets.is_subset(&other.targets);
        let other_is_subset = other.targets.is_subset(&self.targets);
        match (self_is_subset, other_is_subset) {
            (true, true) => ScopeRelation::Identical,
            (true, false) => ScopeRelation::StrictSubset,
            (false, true) => ScopeRelation::StrictSuperset,
            (false, false) => {
                if self.targets.is_disjoint(&other.targets) {
                    ScopeRelation::Disjoint
                } else {
                    ScopeRelation::Incomparable
                }
            }
        }
    }

    /// Reports whether this set contains the exact target pair.
    /// 报告该集合是否包含精确目标组合。
    pub fn contains_target(&self, service_entry_id: &str, credential_profile_id: &str) -> bool {
        self.targets.contains(&(
            service_entry_id.to_string(),
            credential_profile_id.to_string(),
        ))
    }
}

/// Deterministic relation between two normalized rule target sets (plan §6.5).
/// 两个归一化规则目标集合之间的确定性关系（方案 §6.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeRelation {
    /// Identical target sets are duplicate authority and must reject the whole manifest.
    /// 完全相同目标集合属于重复权威，必须拒绝整份清单。
    Identical,
    /// The left set is a strict subset of the right set; the narrower rule wins.
    /// 左集合是右集合的严格子集；更窄的规则胜出。
    StrictSubset,
    /// The right set is a strict subset of the left set; the narrower rule wins.
    /// 右集合是左集合的严格子集；更窄的规则胜出。
    StrictSuperset,
    /// Disjoint target sets coexist without ordering.
    /// 不相交目标集合无需排序即可共存。
    Disjoint,
    /// Intersecting but incomparable target sets must reject the whole manifest.
    /// 相交但不可比较的目标集合必须拒绝整份清单。
    Incomparable,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal manifest with two entries and two credential profiles.
    /// 构建一个含两个入口与两个凭据配置档的最小清单。
    fn manifest_with_services() -> ProviderManifestV1 {
        serde_json::from_value(serde_json::json!({
            "schema_version": "vulcan.provider-manifest.v2",
            "provider_id": "probe-scope",
            "revision": 1,
            "published_at": "2026-08-07T00:00:00Z",
            "engine_contract": { "minimum_manifest_engine": 1 },
            "display": { "name": "Probe Scope", "category": "official" },
            "services": [
                {
                    "service_entry_id": "official-api",
                    "display_name": "Official API",
                    "kind": "conversation",
                    "base_url": "https://api.probe.example.com",
                    "credential_profiles": [
                        { "credential_profile_id": "p1", "display_name": "P1", "credential_mode": "token" },
                        { "credential_profile_id": "p2", "display_name": "P2", "credential_mode": "token" }
                    ],
                    "default_authentication": "bearer_authorization",
                    "protocol_routes": [
                        {
                            "protocol_profile_id": "openai-chat-completions.v1",
                            "endpoint_path": "/v1/chat/completions",
                            "verified": true
                        }
                    ]
                },
                {
                    "service_entry_id": "tools-api",
                    "display_name": "Tools API",
                    "kind": "provider_tools",
                    "base_url": "https://tools.probe.example.com",
                    "credential_profiles": [
                        { "credential_profile_id": "p2", "display_name": "P2", "credential_mode": "token" }
                    ],
                    "default_authentication": "bearer_authorization",
                    "protocol_routes": []
                }
            ],
            "model_defaults": {
                "default_enabled": true,
                "catalog_membership_policy": "declared_only",
                "unknown_model_policy": "declared_rule_required"
            },
            "model_rules": [],
            "provenance": { "status": "migrated", "sources": [] }
        }))
        .expect("fixture should parse")
    }

    /// Verifies empty scopes expand to every entry/profile pair in the trust domain.
    /// 验证空作用域展开为信任域内全部入口/配置档组合。
    #[test]
    fn empty_scope_expands_to_full_trust_domain() {
        let manifest = manifest_with_services();
        let rule: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "global",
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let targets = NormalizedRuleTargets::from_rule(&manifest, &rule);
        assert_eq!(targets.targets.len(), 3);
        assert!(targets.contains_target("official-api", "p1"));
        assert!(targets.contains_target("official-api", "p2"));
        assert!(targets.contains_target("tools-api", "p2"));
    }

    /// Verifies a credential-scoped rule is a strict subset of the global rule.
    /// 验证凭据作用域规则是全局规则的严格子集。
    #[test]
    fn credential_scope_is_strict_subset_of_global() {
        let manifest = manifest_with_services();
        let global: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "global",
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let scoped: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "scoped",
            "scope": { "service_entry_ids": ["official-api"], "credential_profile_ids": ["p1"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-responses.v1"
        }))
        .expect("rule should parse");
        let global_targets = NormalizedRuleTargets::from_rule(&manifest, &global);
        let scoped_targets = NormalizedRuleTargets::from_rule(&manifest, &scoped);
        assert_eq!(
            scoped_targets.relation(&global_targets),
            ScopeRelation::StrictSubset
        );
        assert_eq!(
            global_targets.relation(&scoped_targets),
            ScopeRelation::StrictSuperset
        );
    }

    /// Verifies overlapping but incomparable scopes are detected regardless of order.
    /// 验证相交但不可比较的作用域与顺序无关地被检测。
    #[test]
    fn overlapping_incomparable_scopes_are_detected() {
        let manifest = manifest_with_services();
        let left: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "left",
            "scope": { "service_entry_ids": ["official-api"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let right: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "right",
            "scope": { "credential_profile_ids": ["p2"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let left_targets = NormalizedRuleTargets::from_rule(&manifest, &left);
        let right_targets = NormalizedRuleTargets::from_rule(&manifest, &right);
        assert_eq!(
            left_targets.relation(&right_targets),
            ScopeRelation::Incomparable
        );
        assert_eq!(
            right_targets.relation(&left_targets),
            ScopeRelation::Incomparable
        );
    }

    /// Verifies identical target sets are reported without order dependence.
    /// 验证完全相同目标集合被无顺序依赖地报告。
    #[test]
    fn identical_target_sets_are_duplicate() {
        let manifest = manifest_with_services();
        let first: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "first",
            "scope": { "credential_profile_ids": ["p1", "p2"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let second: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "second",
            "scope": { "credential_profile_ids": ["p2", "p1"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let first_targets = NormalizedRuleTargets::from_rule(&manifest, &first);
        let second_targets = NormalizedRuleTargets::from_rule(&manifest, &second);
        assert_eq!(
            first_targets.relation(&second_targets),
            ScopeRelation::Identical
        );
        assert_eq!(
            second_targets.relation(&first_targets),
            ScopeRelation::Identical
        );
    }

    /// Verifies disjoint scopes coexist.
    /// 验证不相交作用域可共存。
    #[test]
    fn disjoint_scopes_coexist() {
        let manifest = manifest_with_services();
        let first: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "first",
            "scope": { "service_entry_ids": ["official-api"], "credential_profile_ids": ["p1"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let second: ModelRule = serde_json::from_value(serde_json::json!({
            "rule_id": "second",
            "scope": { "service_entry_ids": ["tools-api"] },
            "matcher": { "kind": "exact", "value": "m1" },
            "selected_protocol": "openai-chat-completions.v1"
        }))
        .expect("rule should parse");
        let first_targets = NormalizedRuleTargets::from_rule(&manifest, &first);
        let second_targets = NormalizedRuleTargets::from_rule(&manifest, &second);
        assert_eq!(
            first_targets.relation(&second_targets),
            ScopeRelation::Disjoint
        );
        assert_eq!(
            second_targets.relation(&first_targets),
            ScopeRelation::Disjoint
        );
    }
}
