//! Strict normalized quota-evidence regressions.
//! 严格归一化额度证据回归测试。

use super::*;

/// Verifies quota unlock comparisons accept exact decimal forms and reject malformed evidence.
/// 验证额度解锁比较接受精确十进制形式并拒绝格式错误的证据。
#[test]
fn normalized_usage_decimal_positive_check_is_strict() {
    for value in ["1", "+0.25", ".5", "1e-3"] {
        assert!(normalized_usage_decimal_is_positive(value), "{value}");
    }
    for value in ["0", "0.0", "0e10", "-1", "1e", "abc1", "1.2.3"] {
        assert!(!normalized_usage_decimal_is_positive(value), "{value}");
    }
}

use crate::runtime::PublicAiRuntime;
use crate::runtime::catalog_policy::apply_model_catalog_overlays_with_account_access;
use crate::runtime::test_support::{account_usage, connection, with_protocol_variants};
use crate::{PersistedAiConfiguration, parse_openai_model_catalog};
use std::collections::BTreeSet;
use vulcan_ai_contract::AccountUsageSnapshot;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_RESPONSES_PROFILE_ID,
};

/// Verifies account-plan model access is read from manifest rules instead of provider code.
/// 验证账号套餐模型访问从 Manifest 规则读取而不是来自供应商代码。
#[test]
fn manifest_account_plan_allowlist_filters_catalog_models() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let manifest = runtime
        .system_manifest("kimi")
        .expect("Kimi manifest must exist");
    let mut configuration = PersistedAiConfiguration::default();
    let mut plan_connection = connection("kimi-plan-account");
    plan_connection.provider_id = String::from("kimi");
    plan_connection.service_entry_id = String::from("coding-plan");
    plan_connection.credential_profile_id = String::from("coding-api-key");
    configuration
        .connection_graph
        .connections
        .insert(plan_connection.id.clone(), plan_connection);
    let source = with_protocol_variants(
        parse_openai_model_catalog(
            "kimi-plan-account",
            1,
            r#"{"data":[{"id":"kimi-for-coding"},{"id":"kimi-for-coding-highspeed"},{"id":"k3-256k"},{"id":"k3"},{"id":"future-plan-model"}]}"#,
        )
        .expect("fixture catalog should parse"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        ],
    );
    // moderato_usage is the exact provider evidence consumed by Kimi's configured plan-name policy.
    // moderato_usage 是 Kimi 已配置套餐名称策略消费的精确供应商证据。
    let moderato_usage = AccountUsageSnapshot {
        credential_id: String::from("kimi-plan-account"),
        availability: vulcan_ai_contract::AccountUsageAvailability::Supported,
        plan_name: Some(String::from("Moderato")),
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: Vec::new(),
        observed_at: None,
        refresh_message: None,
    };
    let mut moderato = source.clone();
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "kimi",
        "coding-plan",
        Some(&moderato_usage),
        manifest
            .model_source_policy("coding-plan")
            .expect("Kimi coding-plan source policy must compile"),
        Some(manifest.as_ref()),
        &mut moderato,
    )
    .expect("manifest plan filter should apply");
    let visible = moderato
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(visible.contains("kimi-for-coding"));
    assert!(visible.contains("k3-256k"));
    assert!(visible.contains("future-plan-model"));
    assert!(!visible.contains("kimi-for-coding-highspeed"));
    assert!(!visible.contains("k3"));

    let mut unknown_plan = source;
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "kimi",
        "coding-plan",
        None,
        manifest
            .model_source_policy("coding-plan")
            .expect("Kimi coding-plan source policy must compile"),
        Some(manifest.as_ref()),
        &mut unknown_plan,
    )
    .expect("missing plan evidence must not hide models");
    assert_eq!(unknown_plan.models.len(), 5);
}

/// Verifies CommandCode plan groups, exact exclusions, and quota unlocks are Manifest-driven.
/// 验证 CommandCode 套餐分组、精确排除项与额度解锁均由 Manifest 驱动。
#[test]
fn manifest_account_model_access_projects_commandcode_catalog() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let manifest = runtime
        .system_manifest("commandcode")
        .expect("CommandCode manifest must exist");
    let mut configuration = PersistedAiConfiguration::default();
    let mut plan_connection = connection("commandcode-plan-account");
    plan_connection.provider_id = String::from("commandcode");
    plan_connection.service_entry_id = String::from("provider-api");
    plan_connection.credential_profile_id = String::from("user-api-key");
    configuration
        .connection_graph
        .connections
        .insert(plan_connection.id.clone(), plan_connection);
    let source = with_protocol_variants(
        parse_openai_model_catalog(
            "commandcode-plan-account",
            1,
            r#"{"data":[{"id":"gpt-5.5"},{"id":"deepseek/deepseek-v4-flash"},{"id":"meta/muse-spark-1.2"},{"id":"google/gemini-3.6-flash"},{"id":"claude-fable-5"}]}"#,
        )
        .expect("fixture catalog should parse"),
        &[OPENAI_CHAT_PROFILE_ID, ANTHROPIC_MESSAGES_PROFILE_ID],
    );
    let source_policy = manifest
        .model_source_policy("provider-api")
        .expect("CommandCode source policy must compile");

    let go_usage = account_usage(
        "credential-commandcode-plan-account",
        Some("Go"),
        Some("individual-go"),
        Vec::new(),
    );
    let mut go_catalog = source.clone();
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "commandcode",
        "provider-api",
        Some(&go_usage),
        source_policy,
        Some(manifest.as_ref()),
        &mut go_catalog,
    )
    .expect("Go plan access should apply");
    let go_models = go_catalog
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(go_models.contains("deepseek/deepseek-v4-flash"));
    assert!(go_models.contains("google/gemini-3.6-flash"));
    assert!(!go_models.contains("gpt-5.5"));
    assert!(!go_models.contains("meta/muse-spark-1.2"));
    assert!(!go_models.contains("claude-fable-5"));

    let pro_usage = account_usage(
        "credential-commandcode-plan-account",
        Some("Pro"),
        Some("individual-pro"),
        Vec::new(),
    );
    let mut pro_catalog = source.clone();
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "commandcode",
        "provider-api",
        Some(&pro_usage),
        source_policy,
        Some(manifest.as_ref()),
        &mut pro_catalog,
    )
    .expect("Pro plan access should apply");
    let pro_models = pro_catalog
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(pro_models.contains("gpt-5.5"));
    assert!(pro_models.contains("deepseek/deepseek-v4-flash"));
    assert!(!pro_models.contains("claude-fable-5"));

    let unknown_usage = account_usage(
        "credential-commandcode-plan-account",
        Some("Future"),
        Some("future-plan"),
        Vec::new(),
    );
    let mut unknown_catalog = source.clone();
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "commandcode",
        "provider-api",
        Some(&unknown_usage),
        source_policy,
        Some(manifest.as_ref()),
        &mut unknown_catalog,
    )
    .expect("an unknown plan must retain CommandCode's upstream allow behavior");
    assert_eq!(unknown_catalog.models.len(), 5);

    let purchased_credit = vulcan_ai_contract::AccountUsageWindow {
        id: String::from("purchased-credits"),
        display_name: String::from("Purchased credits"),
        bucket_name: None,
        used: Some(String::from("0")),
        remaining: Some(String::from("0.25")),
        total: Some(String::from("0.25")),
        remaining_percent: None,
        unit: String::from("USD"),
        starts_at: None,
        resets_at: None,
        unlimited: false,
        available: true,
    };
    let mut zero_credit = purchased_credit.clone();
    zero_credit.remaining = Some(String::from("0"));
    let zero_credit_usage = account_usage(
        "credential-commandcode-plan-account",
        Some("Go"),
        Some("individual-go"),
        vec![zero_credit],
    );
    let mut zero_credit_catalog = source.clone();
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "commandcode",
        "provider-api",
        Some(&zero_credit_usage),
        source_policy,
        Some(manifest.as_ref()),
        &mut zero_credit_catalog,
    )
    .expect("zero purchased credits must retain plan filtering");
    let zero_credit_models = zero_credit_catalog
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(zero_credit_models, go_models);

    let unlocked_usage = account_usage(
        "credential-commandcode-plan-account",
        Some("Go"),
        Some("individual-go"),
        vec![purchased_credit],
    );
    let mut unlocked_catalog = source;
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "commandcode",
        "provider-api",
        Some(&unlocked_usage),
        source_policy,
        Some(manifest.as_ref()),
        &mut unlocked_catalog,
    )
    .expect("positive purchased credits should unlock the declared catalog");
    assert_eq!(unlocked_catalog.models.len(), 5);
}

/// Verifies providers without an account-model-access declaration keep their own catalog logic.
/// 验证未声明账号模型访问投影的供应商保持其自身目录逻辑。
#[test]
fn apikeyfan_catalog_does_not_inherit_account_plan_filtering() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let manifest = runtime
        .system_manifest("apikeyfan")
        .expect("APIKEY.FAN manifest must exist");
    let mut configuration = PersistedAiConfiguration::default();
    let mut apikeyfan_connection = connection("apikeyfan-group-account");
    apikeyfan_connection.provider_id = String::from("apikeyfan");
    apikeyfan_connection.service_entry_id = String::from("api");
    apikeyfan_connection.credential_profile_id = String::from("api-key");
    configuration
        .connection_graph
        .connections
        .insert(apikeyfan_connection.id.clone(), apikeyfan_connection);
    let mut catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "apikeyfan-group-account",
            1,
            r#"{"data":[{"id":"deepseek-v4-flash"}]}"#,
        )
        .expect("fixture catalog should parse"),
        &[OPENAI_RESPONSES_PROFILE_ID],
    );
    let unrelated_usage = account_usage(
        "credential-apikeyfan-group-account",
        Some("Go"),
        Some("individual-go"),
        Vec::new(),
    );
    apply_model_catalog_overlays_with_account_access(
        &configuration,
        "apikeyfan",
        "api",
        Some(&unrelated_usage),
        manifest
            .model_source_policy("api")
            .expect("APIKEY.FAN source policy must compile"),
        Some(manifest.as_ref()),
        &mut catalog,
    )
    .expect("APIKEY.FAN catalog projection should remain independent");
    assert_eq!(
        catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["deepseek-v4-flash"]
    );
}
