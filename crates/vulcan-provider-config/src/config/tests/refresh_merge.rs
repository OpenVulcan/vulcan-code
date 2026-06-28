use super::super::{PersistedProviderEntry, merge_refreshed_models, persist_remote_models};
use super::helpers::test_model_choice;

/// Verifies remote catalog refresh only auto-enables newly discovered default models and keeps previously disabled default models disabled.
/// 验证远端目录刷新只会自动启用新发现的默认模型，并保持此前被禁用的默认模型继续处于关闭状态。
#[test]
fn merge_refreshed_models_only_auto_enables_new_default_models() {
    let mut old_default_model = test_model_choice("default-model", true, None);
    old_default_model.default_enabled = true;
    let manual_model = test_model_choice("manual-model", true, None);
    let mut new_default_model = test_model_choice("new-default-model", true, None);
    new_default_model.default_enabled = true;

    let mut entry = PersistedProviderEntry::default();
    entry.models = persist_remote_models(&[old_default_model.clone(), manual_model.clone()]);
    entry.enabled_models = Some(vec![String::from("manual-model")]);

    merge_refreshed_models(
        &mut entry,
        &[old_default_model, manual_model, new_default_model],
    );

    assert_eq!(
        entry.enabled_models,
        Some(vec![
            String::from("manual-model"),
            String::from("new-default-model"),
        ])
    );
}
