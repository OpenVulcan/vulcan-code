//! User-owned subagent display metadata tests.
//! 用户拥有子代理展示元数据测试。

use super::subagent_fixed_model_tests::loaded_definition_with_provider_model;
use super::*;

/// Verifies a saved user agent keeps exactly the submitted name and description and no per-locale metadata.
/// 验证保存的用户智能体只保留提交的名称与说明，不保留按语言的元数据。
#[test]
fn user_definition_save_keeps_one_name_and_drops_locale_metadata() {
    let mut definition = loaded_definition_with_provider_model(None, None);
    definition.locales = BTreeMap::from([(
        String::from("zh-CN"),
        vulcan_task_session::catalog::SubagentDefinitionLocaleDefinition {
            name: Some(String::from("旧名称")),
            description: Some(String::from("旧说明")),
        },
    )]);
    let request = SubagentDefinitionSaveRequest {
        agent_id: String::from("engineering/test-agent"),
        name: String::from("  代码审查员  "),
        description: Some(String::from("审查改动并指出风险。")),
        ..SubagentDefinitionSaveRequest::default()
    };

    apply_user_definition_display_metadata(&mut definition, &request, "engineering/test-agent");

    assert_eq!(definition.name, "代码审查员");
    assert_eq!(
        definition.description.as_deref(),
        Some("审查改动并指出风险。")
    );
    assert!(
        definition.locales.is_empty(),
        "user-owned definitions must not carry per-locale metadata"
    );
}

/// Verifies a blank submitted name falls back to the agent id and a blank description is cleared.
/// 验证提交的名称为空时回退到智能体标识，空说明会被清除。
#[test]
fn user_definition_save_falls_back_to_agent_id_for_blank_name() {
    let mut definition = loaded_definition_with_provider_model(None, None);
    definition.description = Some(String::from("Previous description"));
    let request = SubagentDefinitionSaveRequest {
        agent_id: String::from("engineering/test-agent"),
        name: String::from("   "),
        description: Some(String::from("  ")),
        ..SubagentDefinitionSaveRequest::default()
    };

    apply_user_definition_display_metadata(&mut definition, &request, "engineering/test-agent");

    assert_eq!(definition.name, "engineering/test-agent");
    assert_eq!(definition.description, None);
}
