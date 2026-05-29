//! Developer-prompt rendering for implicit LuaSkills host dispatch.
//! 隐式 LuaSkills 宿主调度的开发者提示词渲染模块。

use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_tools_luaskills_call::{CALL_LUASKILL_TOOL_NAME, CALL_LUASKILL_USAGE_MD};

/// Stable AI-visible LuaSkill help tool name.
/// 稳定的 AI 可见 LuaSkill 帮助工具名称。
pub const LUASKILL_HELP_TOOL_NAME: &str = "luaskill";

/// One prompt-visible LuaSkill entry that can be called through `call_luaskill`.
/// 一条可通过 `call_luaskill` 调用并向提示词暴露的 LuaSkill 入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSkillPromptEntry {
    /// Complete canonical LuaSkills tool name used as the dispatcher target.
    /// 作为调度目标使用的完整 canonical LuaSkills 工具名。
    pub name: String,
    /// Compact AI-facing description of what this LuaSkill entry does.
    /// 面向 AI 的紧凑 LuaSkill 入口用途说明。
    pub description: String,
    /// Optional LuaSkills help flow name that best matches this entry.
    /// 与该入口最匹配的可选 LuaSkills 帮助流程名称。
    pub help_flow: Option<String>,
    /// Optional compact LuaSkills help text rendered from the existing help tree.
    /// 从现有帮助树渲染出的可选紧凑 LuaSkills 帮助文本。
    pub help_summary: Option<String>,
    /// Optional full LuaSkills help text returned only by the `luaskill` detail tool.
    /// 仅由 `luaskill` 详情工具返回的可选完整 LuaSkills 帮助文本。
    pub help_content: Option<String>,
    /// Compact argument schema summary after host-managed fields are hidden.
    /// 隐藏宿主管理字段后的紧凑参数 schema 摘要。
    pub argument_summary: Option<String>,
    /// Structured Markdown schema help after host-managed fields are hidden.
    /// 隐藏宿主管理字段后的结构化 Markdown schema 说明。
    pub schema_markdown: Option<String>,
}

impl LuaSkillPromptEntry {
    /// Builds one normalized prompt entry and drops surrounding whitespace.
    /// 构建一条归一化提示词入口并移除首尾空白。
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            description: description.into().trim().to_string(),
            help_flow: None,
            help_summary: None,
            help_content: None,
            argument_summary: None,
            schema_markdown: None,
        }
    }

    /// Attaches one LuaSkills help summary to this prompt entry.
    /// 为当前提示词入口附加一条 LuaSkills 帮助摘要。
    pub fn with_help_summary(
        mut self,
        help_flow: Option<impl Into<String>>,
        help_summary: Option<impl Into<String>>,
    ) -> Self {
        self.help_flow = help_flow
            .map(Into::into)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.help_summary = help_summary
            .map(Into::into)
            .map(|value| compact_whitespace(&value))
            .filter(|value| !value.is_empty());
        self
    }

    /// Attaches the full LuaSkills help body returned by the dedicated help tool.
    /// 附加由专用帮助工具返回的完整 LuaSkills 帮助正文。
    pub fn with_help_content(mut self, help_content: impl Into<String>) -> Self {
        self.help_content =
            Some(help_content.into().trim().to_string()).filter(|value| !value.is_empty());
        self
    }

    /// Attaches one compact argument schema summary to this prompt entry.
    /// 为当前提示词入口附加一条紧凑参数 schema 摘要。
    pub fn with_argument_summary(mut self, argument_summary: impl Into<String>) -> Self {
        self.argument_summary =
            Some(compact_whitespace(&argument_summary.into())).filter(|value| !value.is_empty());
        self
    }

    /// Attaches structured Markdown schema help to this prompt entry.
    /// 为当前提示词入口附加结构化 Markdown schema 说明。
    pub fn with_schema_markdown(mut self, schema_markdown: impl Into<String>) -> Self {
        self.schema_markdown =
            Some(schema_markdown.into().trim().to_string()).filter(|value| !value.is_empty());
        self
    }

    /// Returns whether this entry has a usable canonical name.
    /// 返回该入口是否具备可用的 canonical 名称。
    fn has_name(&self) -> bool {
        !self.name.trim().is_empty()
    }
}

/// Renders a compact argument summary from one object-shaped JSON Schema.
/// 从对象形 JSON Schema 渲染紧凑参数摘要。
pub fn render_luaskill_argument_summary(schema: &Value) -> Option<String> {
    let properties = schema.get("properties").and_then(Value::as_object)?;
    if properties.is_empty() {
        return Some(String::from("No arguments."));
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut lines = Vec::new();
    for (name, property) in properties {
        let kind = schema_type_label(property);
        let required_label = if required.contains(name) {
            "required"
        } else {
            "optional"
        };
        let description = property
            .get("description")
            .and_then(Value::as_str)
            .map(compact_whitespace)
            .unwrap_or_default();
        if description.is_empty() {
            lines.push(format!("{name}: {kind}, {required_label}."));
        } else {
            lines.push(format!("{name}: {kind}, {required_label}. {description}"));
        }
    }
    Some(lines.join(" "))
}

/// Renders structured Markdown help from one object-shaped JSON Schema.
/// 从对象形 JSON Schema 渲染结构化 Markdown 说明。
pub fn render_luaskill_schema_markdown(schema: &Value) -> Option<String> {
    let mut output = String::new();
    render_schema_node("input", schema, 0, true, &mut output);
    (!output.trim().is_empty()).then(|| output.trim().to_string())
}

/// Renders the developer segment that advertises implicit LuaSkills plus the dispatcher usage rules.
/// 渲染说明隐式 LuaSkills 与调度器使用规则的开发者提示词片段。
pub fn render_luaskills_prompt_segment(entries: &[LuaSkillPromptEntry]) -> Option<String> {
    let mut entries = normalized_sorted_entries(entries);
    if entries.is_empty() {
        return None;
    }

    let mut output = String::from("## LuaSkills Implicit Dispatch\n\n");
    output.push_str("Some LuaSkills are enabled as implicit dispatch targets. They are not exposed as direct function tools, so call them only through `");
    output.push_str(CALL_LUASKILL_TOOL_NAME);
    output.push_str("`.\n\n");
    output.push_str("The list below is discovery-only and intentionally omits argument details. Before calling any hidden LuaSkill, first call `");
    output.push_str(LUASKILL_HELP_TOOL_NAME);
    output.push_str("` with the complete canonical LuaSkill name to get its complete usage help and structured Markdown schema, then call `");
    output.push_str(CALL_LUASKILL_TOOL_NAME);
    output.push_str("`.\n\n");
    output.push_str(CALL_LUASKILL_USAGE_MD.trim());
    output.push_str("\n\nEnabled implicit LuaSkills:\n");
    for entry in entries.drain(..) {
        output.push_str("\n### `");
        output.push_str(&entry.name);
        output.push_str("`\n");
        push_non_empty_field(&mut output, "Description", &entry.description);
        if let Some(help_flow) = entry.help_flow.as_deref() {
            push_non_empty_field(&mut output, "LuaSkills help flow", help_flow);
        }
        if let Some(help_summary) = entry.help_summary.as_deref() {
            push_non_empty_field(&mut output, "LuaSkills help", help_summary);
        }
    }
    Some(output)
}

/// Renders full help output returned by the `luaskill` tool for one enabled implicit entry.
/// 为单个已启用隐式入口渲染 `luaskill` 工具返回的完整帮助输出。
pub fn render_luaskill_help_output(
    requested_name: &str,
    entries: &[LuaSkillPromptEntry],
) -> Result<String, String> {
    let stable_name = requested_name.trim();
    let Some(entry) = entries
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(stable_name))
    else {
        let available = normalized_sorted_entries(entries)
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if available.is_empty() {
            String::from("No implicit LuaSkills are enabled for this execution turn.")
        } else {
            format!("Enabled implicit LuaSkills: {available}.")
        };
        return Err(format!(
            "LuaSkill `{stable_name}` is not enabled for implicit dispatch. {suffix}"
        ));
    };

    let mut output = String::new();
    output.push_str("# LuaSkill Usage\n");
    if let Some(help_summary) = entry.help_summary.as_deref() {
        output.push_str("\n## LuaSkills Help\n\n");
        output.push_str(entry.help_content.as_deref().unwrap_or(help_summary));
        output.push('\n');
    }
    if let Some(schema_markdown) = entry.schema_markdown.as_deref() {
        output.push_str("\n## Schema\n\n");
        output.push_str(schema_markdown);
        output.push('\n');
    } else if let Some(argument_summary) = entry.argument_summary.as_deref() {
        output.push_str("\n## Schema\n\n");
        output.push_str(argument_summary);
        output.push('\n');
    }
    Ok(output)
}

/// Returns normalized entries sorted by canonical tool name with empty names removed.
/// 返回按 canonical 工具名排序且移除空名称后的归一化入口集合。
fn normalized_sorted_entries(entries: &[LuaSkillPromptEntry]) -> Vec<LuaSkillPromptEntry> {
    let mut entries = entries
        .iter()
        .filter(|entry| entry.has_name())
        .cloned()
        .map(|mut entry| {
            entry.name = entry.name.trim().to_string();
            entry.description = compact_whitespace(&entry.description);
            entry.help_summary = entry.help_summary.map(|value| compact_whitespace(&value));
            entry.argument_summary = entry
                .argument_summary
                .map(|value| compact_whitespace(&value));
            entry
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries.dedup_by(|left, right| left.name == right.name);
    entries
}

/// Appends one prompt field only when the value is not empty.
/// 仅当字段值非空时追加一个提示词字段。
fn push_non_empty_field(output: &mut String, label: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    output.push_str("- ");
    output.push_str(label);
    output.push_str(": ");
    output.push_str(value);
    output.push('\n');
}

/// Renders one compact type label from a JSON Schema property.
/// 从 JSON Schema 属性渲染一个紧凑类型标签。
fn schema_type_label(schema: &Value) -> String {
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        return kind.to_string();
    }
    if let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) {
        let types = any_of
            .iter()
            .filter_map(|item| item.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();
        if !types.is_empty() {
            return types.join("|");
        }
    }
    String::from("value")
}

/// Renders one schema node into Markdown lines.
/// 将单个 schema 节点渲染为 Markdown 行。
fn render_schema_node(
    name: &str,
    schema: &Value,
    depth: usize,
    required: bool,
    output: &mut String,
) {
    let indent = "  ".repeat(depth);
    let required_label = if required { "required" } else { "optional" };
    let description = schema
        .get("description")
        .and_then(Value::as_str)
        .map(compact_whitespace)
        .unwrap_or_default();
    output.push_str(&indent);
    output.push_str("- `");
    output.push_str(name);
    output.push_str("` (");
    output.push_str(&schema_type_label(schema));
    output.push_str(", ");
    output.push_str(required_label);
    output.push(')');
    if !description.is_empty() {
        output.push_str(": ");
        output.push_str(&description);
    }
    output.push('\n');

    push_schema_constraints(schema, depth + 1, output);

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let required_names = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        for (property_name, property_schema) in properties {
            render_schema_node(
                property_name,
                property_schema,
                depth + 1,
                required_names.contains(property_name),
                output,
            );
        }
    }

    if let Some(items) = schema.get("items") {
        render_schema_node("[]", items, depth + 1, true, output);
    }
}

/// Appends concise Markdown descriptions for common JSON Schema constraints.
/// 为常见 JSON Schema 约束追加紧凑 Markdown 说明。
fn push_schema_constraints(schema: &Value, depth: usize, output: &mut String) {
    let indent = "  ".repeat(depth);
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        let variants = values
            .iter()
            .map(render_schema_literal)
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&indent);
        output.push_str("- allowed values: ");
        output.push_str(&variants);
        output.push('\n');
    }
    for key in [
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "pattern",
    ] {
        if let Some(value) = schema.get(key) {
            output.push_str(&indent);
            output.push_str("- ");
            output.push_str(key);
            output.push_str(": ");
            output.push_str(&render_schema_literal(value));
            output.push('\n');
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get(key).and_then(Value::as_array) {
            output.push_str(&indent);
            output.push_str("- ");
            output.push_str(key);
            output.push_str(": ");
            output.push_str(&render_schema_union_summary(branches));
            output.push('\n');
        }
    }
}

/// Renders one schema literal into compact Markdown text.
/// 将单个 schema 字面值渲染为紧凑 Markdown 文本。
fn render_schema_literal(value: &Value) -> String {
    value
        .as_str()
        .map(|value| format!("`{value}`"))
        .unwrap_or_else(|| value.to_string())
}

/// Renders one JSON Schema union branch list into compact Markdown text.
/// 将一组 JSON Schema 联合分支渲染为紧凑 Markdown 文本。
fn render_schema_union_summary(branches: &[Value]) -> String {
    branches
        .iter()
        .enumerate()
        .map(|(index, branch)| render_schema_union_branch_summary(index + 1, branch))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Renders one JSON Schema union branch without repeating nested field descriptions.
/// 渲染单个 JSON Schema 联合分支，同时避免重复展开嵌套字段说明。
fn render_schema_union_branch_summary(index: usize, branch: &Value) -> String {
    let kind = schema_type_label(branch);
    let description = branch
        .get("description")
        .and_then(Value::as_str)
        .map(compact_whitespace)
        .unwrap_or_default();
    let mut segments = vec![format!("option {index} is `{kind}`")];
    if !description.is_empty() {
        segments.push(description);
    }
    if let Some(values) = branch.get("enum").and_then(Value::as_array) {
        let variants = values
            .iter()
            .map(render_schema_literal)
            .collect::<Vec<_>>()
            .join(", ");
        segments.push(format!("allowed {variants}"));
    }
    if let Some(required) = branch.get("required").and_then(Value::as_array) {
        let required = required
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !required.is_empty() {
            segments.push(format!(
                "requires {}",
                render_compact_schema_name_list(&required, 8)
            ));
        }
    }
    if let Some(properties) = branch.get("properties").and_then(Value::as_object) {
        let fields = properties.keys().cloned().collect::<Vec<_>>();
        if !fields.is_empty() {
            segments.push(format!(
                "fields {}",
                render_compact_schema_name_list(&fields, 8)
            ));
        }
    }
    if let Some(items) = branch.get("items") {
        segments.push(format!("items `{}`", schema_type_label(items)));
    }
    segments.join(", ")
}

/// Renders a capped backtick-wrapped schema name list to keep union notes token-light.
/// 渲染带上限的反引号 schema 名称列表，保持联合分支说明轻量。
fn render_compact_schema_name_list(names: &[String], max_visible: usize) -> String {
    let mut visible = names
        .iter()
        .take(max_visible)
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>();
    if names.len() > max_visible {
        visible.push(format!("+{} more", names.len() - max_visible));
    }
    visible.join(", ")
}

/// Collapses whitespace in one help string to keep developer prompts compact.
/// 折叠帮助字符串中的空白以保持开发者提示词紧凑。
fn compact_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    //! Tests for implicit LuaSkills prompt rendering.
    //! 隐式 LuaSkills 提示词渲染测试。

    use super::{
        LuaSkillPromptEntry, render_luaskill_argument_summary, render_luaskill_help_output,
        render_luaskill_schema_markdown, render_luaskills_prompt_segment,
    };
    use serde_json::json;

    /// Verifies no segment is emitted when no implicit LuaSkills exist.
    /// 验证不存在隐式 LuaSkills 时不会产出提示词片段。
    #[test]
    fn render_segment_omits_empty_entries() {
        assert!(render_luaskills_prompt_segment(&[]).is_none());
    }

    /// Verifies the prompt segment advertises discovery data but omits argument details.
    /// 验证提示词片段会说明发现信息但省略参数详情。
    #[test]
    fn render_segment_lists_implicit_entries() {
        let rendered = render_luaskills_prompt_segment(&[LuaSkillPromptEntry::new(
            "vulcan-codekit-ast-tree",
            "Inspect source tree structure.",
        )
        .with_help_summary(Some("main"), Some("Pick candidate source files first."))
        .with_argument_summary("paths: string, required.")])
        .expect("segment should render");

        assert!(rendered.contains("call_luaskill"));
        assert!(rendered.contains("first call `luaskill`"));
        assert!(rendered.contains("vulcan-codekit-ast-tree"));
        assert!(rendered.contains("Pick candidate source files first."));
        assert!(!rendered.contains("paths: string, required."));
    }

    /// Verifies compact schema summaries hide JSON shape noise.
    /// 验证紧凑 schema 摘要会隐藏 JSON 结构噪音。
    #[test]
    fn render_argument_summary_compacts_properties() {
        let summary = render_luaskill_argument_summary(&json!({
            "type": "object",
            "properties": {
                "paths": {
                    "type": "string",
                    "description": "Exactly one directory path."
                },
                "noignore": {
                    "type": "boolean"
                }
            },
            "required": ["paths"]
        }))
        .expect("summary should render");

        assert!(summary.contains("paths: string, required."));
        assert!(summary.contains("noignore: boolean, optional."));
    }

    /// Verifies LuaSkill help output is scoped to enabled implicit entries.
    /// 验证 LuaSkill 帮助输出受启用隐式入口限制。
    #[test]
    fn render_help_output_requires_enabled_entry() {
        let entries = [LuaSkillPromptEntry::new("vulcan-curl-get", "Run HTTP GET.")
            .with_help_summary(Some("main"), Some("HTTP request helper."))
            .with_schema_markdown("- `url` (string, required): Target URL.")];

        let output =
            render_luaskill_help_output("vulcan-curl-get", &entries).expect("help should render");
        assert!(output.starts_with("# LuaSkill Usage"));
        assert!(!output.contains("- Name:"));
        assert!(!output.contains("Description:"));
        assert!(!output.contains("LuaSkills help flow"));
        assert!(output.contains("## Schema"));
        assert!(output.contains("Target URL"));
        assert!(!output.contains("## Dispatch Rule"));
        assert!(render_luaskill_help_output("missing", &entries).is_err());
    }

    /// Verifies complex schema branches are rendered as Markdown instead of raw JSON.
    /// 验证复杂 schema 分支会渲染为 Markdown 而不是原始 JSON。
    #[test]
    fn render_schema_markdown_describes_union_branches() {
        let rendered = render_luaskill_schema_markdown(&json!({
            "type": "object",
            "properties": {
                "range": {
                    "description": "Expected line range.",
                    "anyOf": [
                        { "type": "string", "description": "Compact range." },
                        {
                            "type": "object",
                            "properties": {
                                "start_line": { "type": "integer", "minimum": 1 },
                                "end_line": { "type": "integer", "minimum": 1 }
                            },
                            "required": ["start_line", "end_line"]
                        }
                    ]
                }
            }
        }))
        .expect("schema markdown should render");

        assert!(rendered.contains("anyOf"));
        assert!(rendered.contains("option 1 is `string`"));
        assert!(rendered.contains("requires `start_line`, `end_line`"));
        assert!(rendered.contains("fields `end_line`, `start_line`"));
        assert!(rendered.contains("`range`"));
    }
}
