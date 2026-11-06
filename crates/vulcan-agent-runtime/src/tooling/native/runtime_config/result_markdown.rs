//! Read-only conversation projections of the upstream configuration contract.
//! 上游配置契约的只读会话投影。

use super::{NativeToolError, RuntimeConfigAction, VulcanToolInvocationResult};
use crate::tooling::native::RUNTIME_CONFIG_TOOL_ID;
use luaskills::{
    RuntimeSkillConfigToolResponse, RuntimeSkillPackageConfigDescriptor,
    RuntimeSkillPackageConfigItemDescriptor, RuntimeSkillPackageConfigStatus,
};
use serde_json::Value;
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Decodes the official dispatcher envelope and projects only the authorized reading action.
/// 解码正式分发包络，仅投影获授权的阅读动作。
/// Inputs are the accepted action and upstream JSON; returns shared Markdown or explicit failure.
/// 输入为已接受动作和上游 JSON；返回共享 Markdown 或明确失败。
pub(super) fn project(
    action: RuntimeConfigAction,
    response: &str,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let response: RuntimeSkillConfigToolResponse =
        serde_json::from_str(response).map_err(decode_error)?;
    if !response.ok {
        let error = response
            .error
            .ok_or_else(|| NativeToolError::new("runtime_config failed without an error record"))?;
        return Ok(VulcanToolInvocationResult::from_tool_error_details(
            format!("{}: {}", error.code, error.message),
            Value::Object(error.details),
        ));
    }
    let result = response
        .result
        .ok_or_else(|| NativeToolError::new("runtime_config succeeded without a result"))?;
    let markdown = match action {
        RuntimeConfigAction::Describe => {
            describe(serde_json::from_value(result).map_err(decode_error)?)
        }
        RuntimeConfigAction::Validate => {
            validate(serde_json::from_value(result).map_err(decode_error)?)
        }
    };
    Ok(VulcanToolInvocationResult::from_model_output(
        Value::String(markdown),
    ))
}

/// Converts a decoder failure to the native error boundary without guessing another result shape.
/// 将解码失败转换至原生错误边界，不猜测另一种结果形状。
/// Accepts the serde error and returns a diagnostic without echoing sensitive response data.
/// 接受 serde 错误，返回不回显敏感响应数据的诊断。
fn decode_error(error: serde_json::Error) -> NativeToolError {
    NativeToolError::new(format!("Invalid runtime_config response: {error}"))
}

/// Renders effective package declarations, omitting private snapshots and configured values.
/// 渲染生效包声明，省略私有快照和已配置值。
/// Accepts official typed descriptors and returns all calling constraints and validation facts.
/// 接受正式强类型描述，返回全部调用约束和校验事实。
fn describe(packages: Vec<RuntimeSkillPackageConfigDescriptor>) -> String {
    let mut output = String::from(
        "# Configuration declarations\n\nRead-only. Configured values are not disclosed.\n",
    );
    if packages.is_empty() {
        output.push_str("\nNo effective package declarations.\n");
    }
    for package in packages {
        output.push_str(&format!(
            "\n## {}\n\nComplete: {}.\n",
            inline_code(&package.skill_id),
            package.complete
        ));
        for item in package.items {
            describe_item(&mut output, &item);
        }
        if package.business_issue_count > 0 {
            output.push_str(&format!("\n{} cross-field validation issue(s). Call `{RUNTIME_CONFIG_TOOL_ID}` with `action: validate` and this skill ID for details.\n", package.business_issue_count));
        }
        if !package.orphaned.is_empty() {
            output.push_str(&format!(
                "\nUndeclared stored keys: {}.\n",
                package
                    .orphaned
                    .iter()
                    .map(|key| inline_code(key))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    output
}

/// Appends one declaration with exact constraints and no configured value or host layout hints.
/// 追加单项声明及精确约束，不包含已配置值或宿主布局提示。
/// Mutates the Markdown buffer using the upstream typed descriptor; returns nothing.
/// 使用上游强类型描述修改 Markdown 缓冲；无返回值。
fn describe_item(output: &mut String, item: &RuntimeSkillPackageConfigItemDescriptor) {
    let state = serde_json::to_value(item.state).expect("fieldless configuration state serializes");
    output.push_str(&format!(
        "\n### {}\n\n{}\n\nType: {}; required: {}; state: {}; satisfied: {}.\n",
        inline_code(&item.key),
        item.description,
        item.value_type.as_str(),
        item.required,
        state.as_str().expect("state is a string"),
        item.satisfied
    ));
    if item.sensitive {
        output.push_str("Sensitive setting.\n");
    }
    if item.restart_required {
        output.push_str("Changing this setting may require a restart.\n");
    }
    if item.deprecated {
        output.push_str("Deprecated.\n");
    }
    if let Some(message) = &item.deprecation_message {
        output.push_str(&format!("{message}\n"));
    }
    if let Some(value) = &item.constraints.minimum {
        output.push_str(&format!("- Inclusive minimum: {value}.\n"));
    }
    if let Some(value) = &item.constraints.maximum {
        output.push_str(&format!("- Inclusive maximum: {value}.\n"));
    }
    if let Some(value) = item.constraints.min_length {
        output.push_str(&format!(
            "- Minimum length: {value} Unicode scalar values.\n"
        ));
    }
    if let Some(value) = item.constraints.max_length {
        output.push_str(&format!(
            "- Maximum length: {value} Unicode scalar values.\n"
        ));
    }
    for option in &item.options {
        output.push_str(&format!(
            "- Option {}: {} — {}\n",
            inline_code(&option.value),
            option.label,
            option.description
        ));
    }
    // Defaults are declaration constants explicitly safe to disclose in the upstream contract.
    // 默认值是上游契约明确允许披露的声明常量。
    if let Some(default) = &item.default {
        output.push_str(&format!(
            "\nDeclared default:\n{}\n",
            fenced_code(&default.to_string())
        ));
    }
    if let Some(error) = &item.validation_error {
        output.push_str(&format!(
            "\nValidation: {} — {}\n",
            inline_code(&error.code),
            error.message
        ));
    }
}

/// Renders validation issues while retaining false completeness and undeclared-key facts.
/// 渲染校验问题，保留不完整状态与未声明键事实。
/// Accepts the official status; returns one shared Markdown report without store revisions.
/// 接受正式状态；返回不包含存储修订信息的共享 Markdown 报告。
fn validate(status: RuntimeSkillPackageConfigStatus) -> String {
    let mut output = format!(
        "# Configuration validation: {}\n\nComplete: {}. No configuration was changed.\n",
        inline_code(&status.skill_id),
        status.complete
    );
    for (title, issues) in [("Missing", status.missing), ("Invalid", status.invalid)] {
        for issue in issues {
            output.push_str(&format!(
                "- {title} {}: {} — {}\n",
                inline_code(&issue.key),
                inline_code(&issue.code),
                issue.message
            ));
        }
    }
    for issue in status.business_issues {
        let key = issue
            .key
            .map(|key| format!(" {}", inline_code(&key)))
            .unwrap_or_default();
        output.push_str(&format!(
            "- Cross-field issue{key}: {} — {}\n",
            inline_code(&issue.code),
            issue.message
        ));
    }
    for key in status.orphaned {
        output.push_str(&format!(
            "- Undeclared stored key: {}.\n",
            inline_code(&key)
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Ensures official validation failures remain actionable while internal revisions disappear.
    /// 确保正式校验失败仍可操作，同时移除内部修订信息。
    #[test]
    fn validation_preserves_missing_and_cross_field_issues() {
        let result = project(RuntimeConfigAction::Validate, &json!({"ok":true,"action":"validate","error":null,"result":{
            "skill_id":"example","complete":false,"revision":"private-revision","store_scope":"private-store",
            "missing":[{"key":"token","code":"missing","message":"required"}],"invalid":[],
            "business_issues":[{"code":"example.conflict","message":"conflicting options"}],"orphaned":["old_key"],"orphaned_count":1
        }}).to_string()).expect("valid official response");
        let text = result.model_output.as_str().expect("Markdown");
        for required in ["Complete: false", "token", "example.conflict", "old_key"] {
            assert!(text.contains(required));
        }
        assert!(!text.contains("private-"));
        assert!(result.host_result.is_none());
    }

    /// Ensures returned configured values and host display metadata never reach the model.
    /// 确保返回的已配置值和宿主显示元数据永不进入模型。
    #[test]
    fn describe_excludes_configured_values_and_preserves_constraints() {
        let result = project(RuntimeConfigAction::Describe, &json!({"ok":true,"action":"describe","error":null,"result":[{
            "skill_id":"example","skill_version":"1","complete":true,"revision":"private-revision","store_scope":"private-store",
            "missing_count":0,"invalid_count":0,"business_issue_count":0,"orphaned_count":0,"orphaned":[],
            "items":[{"key":"token","type":"string","required":true,"sensitive":true,"description":"Access token",
                "constraints":{"min_length":1,"max_length":100},"options":[],"advanced":false,"restart_required":true,"deprecated":false,
                "state":"configured","satisfied":true,"value":"SECRET_VALUE","group":"host-only-group"}]
        }]}).to_string()).expect("valid official response");
        let text = result.model_output.as_str().expect("Markdown");
        assert!(text.contains("Maximum length: 100"));
        assert!(text.contains("state: configured"));
        for private in ["SECRET_VALUE", "host-only-group", "private-"] {
            assert!(!text.contains(private));
        }
    }
}
