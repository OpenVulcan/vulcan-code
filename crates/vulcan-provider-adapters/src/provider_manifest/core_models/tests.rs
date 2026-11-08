//! Behaviour checks for the core model list and the provider rules that reference it.
//! 核心模型清单及引用它的供应商规则的行为校验。

use serde_json::{Value, json};

use super::*;
use crate::provider_manifest::{
    FieldSourceKind, ManifestOwnership, ManifestSourceLayer, ResolvedProviderManifest,
    compile_manifest_text_with_core,
};

/// Revision every synthetic core list in these tests carries.
/// 这些测试中每份合成核心清单携带的修订号。
pub(crate) const TEST_CORE_REVISION: u64 = 7;

/// Builds one core entry with the given identity and capabilities.
/// 以给定身份与能力构造一个核心条目。
pub(crate) fn core_entry(model_id: &str, capabilities: Value) -> Value {
    json!({
        "model_id": model_id,
        "display_name": model_id,
        "vendor": "example",
        "capabilities": capabilities,
        "verification": "seeded",
    })
}

/// Renders a core list document around the given entries.
/// 用给定条目渲染一份核心清单文档。
pub(crate) fn core_text(models: Value) -> String {
    json!({
        "schema_version": CORE_MODELS_SCHEMA_VERSION,
        "revision": TEST_CORE_REVISION,
        "published_at": "2026-11-08T00:00:00Z",
        "models": models,
        "provenance": {"status": "migrated", "sources": [{"url": "builtin:test.core"}]},
    })
    .to_string()
}

/// Compiles a core list that must be valid.
/// 编译一份必须有效的核心清单。
pub(crate) fn core_catalog(models: Value) -> CoreModelCatalog {
    compile_core_models_text(&core_text(models)).expect("test core list must compile")
}

/// Returns the failure code of a core list that must be rejected.
/// 返回一份必须被拒绝的核心清单的失败代码。
fn core_rejection(models: Value) -> &'static str {
    compile_core_models_text(&core_text(models))
        .expect_err("core list must be rejected")
        .code
}

/// Returns the checked-in DeepSeek manifest, whose rules reference `deepseek-flash` and
/// `deepseek-v4-pro` and declare only route fields of their own.
/// 返回检入的 DeepSeek 清单，其规则引用 `deepseek-flash` 与 `deepseek-v4-pro`，且自身只声明路线字段。
pub(crate) fn deepseek_manifest() -> Value {
    let path =
        vulcan_runtime_assets::source_bundled_provider_manifests_directory().join("deepseek.json");
    let text = std::fs::read_to_string(&path).expect("bundled deepseek manifest must read");
    serde_json::from_str(&text).expect("bundled deepseek manifest must parse")
}

/// Returns the mutable rule of `manifest` that references `model_id`.
/// 返回 `manifest` 中引用 `model_id` 的可变规则。
fn rule_referencing<'a>(manifest: &'a mut Value, model_id: &str) -> &'a mut Value {
    manifest["model_rules"]
        .as_array_mut()
        .expect("model_rules array")
        .iter_mut()
        .find(|rule| rule["core_model_ref"] == model_id)
        .unwrap_or_else(|| panic!("a rule must reference `{model_id}`"))
}

/// Compiles `manifest` against `core` as a bundled system manifest.
/// 以随包系统清单身份对照 `core` 编译 `manifest`。
pub(crate) fn compile_against(
    manifest: &Value,
    core: &CoreModelCatalog,
) -> Result<ResolvedProviderManifest, ManifestValidationError> {
    compile_manifest_text_with_core(
        &manifest.to_string(),
        ManifestOwnership::System,
        ManifestSourceLayer::Bundled,
        Some(core),
    )
}

/// The synthetic core used for the DeepSeek reference checks.
/// 用于 DeepSeek 引用校验的合成核心清单。
pub(crate) fn deepseek_core() -> CoreModelCatalog {
    core_catalog(json!([
        core_entry(
            "deepseek-flash",
            json!({"context_limit": 1000, "output_limit": 100, "reasoning": "native"})
        ),
        core_entry("deepseek-v4-pro", json!({"context_limit": 2000})),
    ]))
}

#[test]
fn rules_inherit_core_fields_with_core_provenance() {
    let mut manifest = deepseek_manifest();
    manifest["model_defaults"]["capabilities"]["context_limit"] = json!(5);
    let resolved = compile_against(&manifest, &deepseek_core()).expect("manifest must compile");
    assert_eq!(resolved.core_models_revision, Some(TEST_CORE_REVISION));
    let instance = resolved
        .rule_instances
        .iter()
        .find(|instance| {
            instance
                .capability_provenance
                .get("context_limit")
                .and_then(|origin| origin.core_model_id.as_deref())
                == Some("deepseek-flash")
        })
        .expect("a rule instance must inherit the deepseek-flash core entry");
    let capabilities = instance
        .capabilities
        .as_ref()
        .expect("compiled capabilities");
    // The core entry sits above provider defaults and below the rule.
    // 核心条目位于供应商默认值之上、规则之下。
    assert_eq!(capabilities.context_limit, Some(1000));
    assert_eq!(capabilities.output_limit, Some(100));
    let origin = &instance.capability_provenance["context_limit"];
    assert_eq!(origin.source, FieldSourceKind::CoreModel);
    assert_eq!(origin.manifest_revision, TEST_CORE_REVISION);
    assert!(
        origin
            .overridden_sources
            .iter()
            .any(|overridden| overridden.source == FieldSourceKind::ProviderDefaults),
        "the replaced provider default must stay visible in provenance"
    );
    // A field the core leaves undeclared still comes from provider defaults.
    // 核心未声明的字段仍来自供应商默认值。
    assert_eq!(
        instance.capability_provenance["operations"].source,
        FieldSourceKind::ProviderDefaults
    );
}

#[test]
fn an_evidenced_rule_override_wins_over_the_core() {
    let mut manifest = deepseek_manifest();
    rule_referencing(&mut manifest, "deepseek-flash")["capabilities"]["context_limit"] =
        json!(3000);
    let resolved = compile_against(&manifest, &deepseek_core()).expect("manifest must compile");
    let instance = resolved
        .rule_instances
        .iter()
        .find(|instance| {
            instance
                .capabilities
                .as_ref()
                .is_some_and(|caps| caps.context_limit == Some(3000))
        })
        .expect("the override must reach the compiled rule");
    let origin = &instance.capability_provenance["context_limit"];
    assert_ne!(origin.source, FieldSourceKind::CoreModel);
    assert!(
        origin
            .overridden_sources
            .iter()
            .any(|overridden| overridden.source == FieldSourceKind::CoreModel)
    );
}

#[test]
fn restating_an_inherited_core_value_is_rejected() {
    let mut manifest = deepseek_manifest();
    rule_referencing(&mut manifest, "deepseek-flash")["capabilities"]["context_limit"] =
        json!(1000);
    let error =
        compile_against(&manifest, &deepseek_core()).expect_err("redundant override must fail");
    assert_eq!(error.code, "redundant_core_override");
}

#[test]
fn overriding_a_core_value_requires_evidence() {
    let mut manifest = deepseek_manifest();
    let rule = rule_referencing(&mut manifest, "deepseek-flash");
    rule.as_object_mut()
        .expect("rule object")
        .remove("provenance_ref");
    let capabilities = rule["capabilities"]
        .as_object_mut()
        .expect("rule capabilities");
    capabilities.remove("evidence_revision");
    capabilities.insert(String::from("context_limit"), json!(3000));
    let error =
        compile_against(&manifest, &deepseek_core()).expect_err("unevidenced override must fail");
    assert_eq!(error.code, "core_override_without_evidence");
}

#[test]
fn a_reference_must_name_a_core_model() {
    let mut manifest = deepseek_manifest();
    rule_referencing(&mut manifest, "deepseek-flash")["core_model_ref"] = json!("missing-model");
    let error =
        compile_against(&manifest, &deepseek_core()).expect_err("dangling reference must fail");
    assert_eq!(error.code, "unknown_core_model_ref");

    rule_referencing(&mut manifest, "missing-model")["core_model_ref"] = json!("Not An Identifier");
    let error =
        compile_against(&manifest, &deepseek_core()).expect_err("malformed reference must fail");
    assert_eq!(error.code, "invalid_core_model_ref");
}

#[test]
fn core_entries_reject_route_fields_nulls_and_inconsistent_limits() {
    assert_eq!(
        core_rejection(json!([core_entry("m", json!({"evidence_revision": "x"}))])),
        "core_model_route_field"
    );
    assert_eq!(
        core_rejection(json!([core_entry("m", json!({"context_limit": null}))])),
        "core_model_null_capability"
    );
    assert_eq!(
        core_rejection(json!([core_entry("m", json!({"context_limit": 0}))])),
        "core_model_invalid_limit"
    );
    assert_eq!(
        core_rejection(json!([core_entry(
            "m",
            json!({"context_limit": 10, "output_limit": 20})
        )])),
        "core_model_output_exceeds_context"
    );
}

#[test]
fn identities_and_aliases_must_be_unique() {
    let mut aliased = core_entry("model-a", json!({}));
    aliased["aliases"] = json!(["lab: model a", "model-b"]);
    assert_eq!(
        core_rejection(json!([aliased.clone(), core_entry("model-b", json!({}))])),
        "core_model_ambiguous_alias"
    );
    assert_eq!(
        core_rejection(json!([
            core_entry("model-a", json!({})),
            core_entry("model-a", json!({}))
        ])),
        "core_model_duplicate_identity"
    );
    let mut padded = core_entry("model-a", json!({}));
    padded["aliases"] = json!([" Model A"]);
    assert_eq!(core_rejection(json!([padded])), "core_model_invalid_alias");

    // Display spellings are valid aliases once normalized.
    // 规范化后的展示拼写是合法别名。
    aliased["aliases"] = json!(["lab: model a"]);
    let catalog = core_catalog(json!([aliased]));
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog.revision(), TEST_CORE_REVISION);
}
