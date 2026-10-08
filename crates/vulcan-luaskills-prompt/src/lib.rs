//! Developer-prompt and help-tool rendering for enabled LuaSkills packages.
//! 已启用 LuaSkills 包的开发者提示词与帮助工具渲染模块。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;

/// Stable AI-visible LuaSkills help tool name.
/// 稳定的 AI 可见 LuaSkills 帮助工具名称。
pub const LUASKILL_HELP_TOOL_NAME: &str = "luaskill_help";

/// Reserved LuaSkills help flow name of every package's main help node.
/// 每个包主帮助节点使用的 LuaSkills 保留帮助流程名。
pub const LUASKILL_MAIN_HELP_FLOW: &str = "main";

/// Largest main help body injected inline; larger bodies are replaced by their description and topic index.
/// 内联注入的主帮助正文上限；更大的正文改为注入其说明与主题索引。
pub const LUASKILL_MAIN_HELP_INLINE_LIMIT_BYTES: usize = 4 * 1024;

/// How one enabled LuaSkills entry is registered for the model.
/// 单个已启用 LuaSkills 入口面向模型的注册方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaSkillRegistration {
    /// The entry is a direct function tool with a provider-visible schema.
    /// 入口是带 Provider 可见 Schema 的直接函数工具。
    Direct,
    /// The entry hides its schema and is reached through `call_luaskill`.
    /// 入口隐藏 Schema，并通过 `call_luaskill` 调用。
    Implicit,
}

/// One enabled LuaSkills entry inside a package help tree.
/// 包帮助树中的单个已启用 LuaSkills 入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSkillPromptEntry {
    /// Complete canonical LuaSkills entry name.
    /// 完整 canonical LuaSkills 入口名。
    pub name: String,
    /// Compact AI-facing description of what this entry does.
    /// 面向 AI 的紧凑入口用途说明。
    pub description: String,
    /// Registration mode selected by the user.
    /// 用户选择的注册方式。
    pub registration: LuaSkillRegistration,
    /// Help topic declared for this entry, or `None` when the entry is covered by main help.
    /// 当前入口声明的帮助主题；由主帮助覆盖时为 `None`。
    pub help_topic: Option<String>,
    /// Structured Markdown schema help after host-managed fields are hidden.
    /// 隐藏宿主管理字段后的结构化 Markdown schema 说明。
    pub schema_markdown: Option<String>,
}

impl LuaSkillPromptEntry {
    /// Builds one normalized entry and drops surrounding whitespace.
    /// 构建一条归一化入口并移除首尾空白。
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        registration: LuaSkillRegistration,
    ) -> Self {
        Self {
            name: name.into().trim().to_string(),
            description: compact_whitespace(&description.into()),
            registration,
            help_topic: None,
            schema_markdown: None,
        }
    }

    /// Attaches the help topic declared for this entry; the main flow name means no dedicated topic.
    /// 附加当前入口声明的帮助主题；主流程名表示没有专属主题。
    pub fn with_help_topic(mut self, help_topic: impl Into<String>) -> Self {
        self.help_topic = Some(help_topic.into().trim().to_string())
            .filter(|value| !value.is_empty() && value != LUASKILL_MAIN_HELP_FLOW);
        self
    }

    /// Attaches structured Markdown schema help to this entry.
    /// 为当前入口附加结构化 Markdown schema 说明。
    pub fn with_schema_markdown(mut self, schema_markdown: impl Into<String>) -> Self {
        self.schema_markdown =
            Some(schema_markdown.into().trim().to_string()).filter(|value| !value.is_empty());
        self
    }
}

/// One rendered LuaSkills help topic.
/// 一个已渲染的 LuaSkills 帮助主题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSkillHelpTopic {
    /// Topic flow name declared by the package.
    /// 包声明的主题流程名。
    pub name: String,
    /// One-line topic description.
    /// 单行主题说明。
    pub description: String,
    /// Rendered Markdown topic body.
    /// 已渲染的 Markdown 主题正文。
    pub content: String,
}

/// Rendered help tree of one LuaSkills package plus its enabled entries.
/// 单个 LuaSkills 包的已渲染帮助树及其已启用入口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaSkillPackageHelp {
    /// Stable skill identifier used as the package name.
    /// 作为包名使用的稳定技能标识。
    pub skill_id: String,
    /// One-line main help description.
    /// 单行主帮助说明。
    pub description: String,
    /// Rendered Markdown main help body; empty when the package declares none or rendering failed.
    /// 已渲染的 Markdown 主帮助正文；包未声明或渲染失败时为空。
    pub main_content: String,
    /// Rendered help topics in declaration order.
    /// 按声明顺序排列的已渲染帮助主题。
    pub topics: Vec<LuaSkillHelpTopic>,
    /// Enabled entries sorted by canonical name.
    /// 按 canonical 名称排序的已启用入口。
    pub entries: Vec<LuaSkillPromptEntry>,
}

impl LuaSkillPackageHelp {
    /// Builds one package help tree and sorts its entries by canonical name.
    /// 构建单个包帮助树，并按 canonical 名称排序其入口。
    pub fn new(
        skill_id: impl Into<String>,
        description: impl Into<String>,
        main_content: impl Into<String>,
        topics: Vec<LuaSkillHelpTopic>,
        mut entries: Vec<LuaSkillPromptEntry>,
    ) -> Self {
        entries.retain(|entry| !entry.name.is_empty());
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        entries.dedup_by(|left, right| left.name == right.name);
        Self {
            skill_id: skill_id.into().trim().to_string(),
            description: compact_whitespace(&description.into()),
            main_content: main_content.into().trim().to_string(),
            topics,
            entries,
        }
    }

    /// Returns the rendered topic with the given name, ignoring ASCII case.
    /// 返回指定名称的已渲染主题，忽略 ASCII 大小写。
    fn topic(&self, name: &str) -> Option<&LuaSkillHelpTopic> {
        self.topics
            .iter()
            .find(|topic| topic.name.eq_ignore_ascii_case(name))
    }
}

/// Request-scoped routes that decide how each entry is invoked by the model.
/// 决定模型如何调用每个入口的请求级路由。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LuaSkillInvocationRoutes {
    /// Provider-visible function tool name keyed by canonical entry name.
    /// 以 canonical 入口名为键的 Provider 可见函数工具名。
    pub function_tools: BTreeMap<String, String>,
    /// Whether `call_luaskill` is visible for implicit entries.
    /// `call_luaskill` 是否对隐式入口可见。
    pub dispatcher_visible: bool,
    /// Whether `luaskill_help` is visible.
    /// `luaskill_help` 是否可见。
    pub help_tool_visible: bool,
}

/// Parsed arguments of the `luaskill_help` tool.
/// `luaskill_help` 工具解析后的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaSkillHelpRequest {
    /// Main help plus topic index of one package.
    /// 单个包的主帮助与主题索引。
    Package {
        /// Requested skill identifier.
        /// 请求的技能标识。
        skill: String,
    },
    /// One topic of one package.
    /// 单个包的单个主题。
    Topic {
        /// Requested skill identifier.
        /// 请求的技能标识。
        skill: String,
        /// Requested topic name.
        /// 请求的主题名。
        topic: String,
    },
    /// Help topic of one entry, plus its schema when the entry is implicit.
    /// 单个入口的帮助主题；隐式入口另附其 Schema。
    Entry {
        /// Complete canonical entry name.
        /// 完整 canonical 入口名。
        name: String,
    },
}

/// Builds the JSON schema accepted by the `luaskill_help` tool.
/// 构建 `luaskill_help` 工具接受的 JSON Schema。
pub fn luaskill_help_args_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "skill": {
                "type": "string",
                "description": "LuaSkills package name, such as vulcan-codekit. Alone it returns the package guide and topic index."
            },
            "topic": {
                "type": "string",
                "description": "Help topic name from the package topic index. Requires skill."
            },
            "name": {
                "type": "string",
                "description": "Complete canonical LuaSkills entry name, such as vulcan-codekit-rg. Use instead of skill."
            }
        }
    })
}

/// Parses `luaskill_help` arguments into one request; exactly one of `skill` or `name` is required.
/// 将 `luaskill_help` 参数解析为单个请求；必须且只能提供 `skill` 或 `name` 之一。
pub fn parse_luaskill_help_request(args: &Value) -> Result<LuaSkillHelpRequest, String> {
    let Value::Object(object) = args else {
        return Err(format!(
            "`{LUASKILL_HELP_TOOL_NAME}` arguments must be a JSON object."
        ));
    };
    let field = |key: &str| -> Result<Option<String>, String> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => {
                Ok(Some(value.trim().to_string()).filter(|value| !value.is_empty()))
            }
            Some(_) => Err(format!(
                "`{LUASKILL_HELP_TOOL_NAME}.{key}` must be a string."
            )),
        }
    };
    match (field("skill")?, field("topic")?, field("name")?) {
        (Some(skill), None, None) => Ok(LuaSkillHelpRequest::Package { skill }),
        (Some(skill), Some(topic), None) => Ok(LuaSkillHelpRequest::Topic { skill, topic }),
        (None, None, Some(name)) => Ok(LuaSkillHelpRequest::Entry { name }),
        _ => Err(format!(
            "`{LUASKILL_HELP_TOOL_NAME}` accepts either `skill` (optionally with `topic`) or `name`."
        )),
    }
}

/// Renders the developer segment of one package: its main help plus a host-generated invocation block.
/// 渲染单个包的开发者片段：主帮助与宿主生成的调用说明。
///
/// # Parameters
/// 参数
/// - `package`: Package help tree already limited to request-visible entries.
/// - `package`：已限定为请求可见入口的包帮助树。
/// - `routes`: Final provider-visible invocation routes.
/// - `routes`：最终 Provider 可见调用路由。
///
/// # Returns
/// 返回值
/// The segment body, or `None` when no entry of the package is invocable.
/// 片段正文；包内没有可调用入口时返回 `None`。
pub fn render_luaskill_package_segment(
    package: &LuaSkillPackageHelp,
    routes: &LuaSkillInvocationRoutes,
) -> Option<String> {
    let direct = package
        .entries
        .iter()
        .filter_map(|entry| {
            routes
                .function_tools
                .get(&entry.name)
                .map(|tool_name| (entry, tool_name.as_str()))
        })
        .collect::<Vec<_>>();
    let implicit = package
        .entries
        .iter()
        .filter(|entry| {
            entry.registration == LuaSkillRegistration::Implicit
                && routes.dispatcher_visible
                && routes.help_tool_visible
        })
        .collect::<Vec<_>>();
    if direct.is_empty() && implicit.is_empty() {
        return None;
    }

    let skill_id = package.skill_id.as_str();
    let mut output = format!("## LuaSkills Package `{skill_id}`\n\n");
    let main_content = package.main_content.as_str();
    if !main_content.is_empty() && main_content.len() <= LUASKILL_MAIN_HELP_INLINE_LIMIT_BYTES {
        output.push_str(main_content);
        output.push_str("\n\n");
    } else {
        if !package.description.is_empty() {
            output.push_str(&package.description);
            output.push_str("\n\n");
        }
        if !main_content.is_empty() && routes.help_tool_visible {
            output.push_str(&format!(
                "Read the package guide with `{LUASKILL_HELP_TOOL_NAME}` and `{{\"skill\":\"{skill_id}\"}}` before first use.\n\n"
            ));
        }
    }

    output.push_str(&format!("### Invoking `{skill_id}`\n\n"));
    output.push_str(
        "Help text names entries by their canonical LuaSkills names. Invoke them as listed here.\n",
    );
    if !direct.is_empty() {
        output.push_str("\nDirect function tools:\n");
        for (entry, tool_name) in direct {
            output.push_str(&format!("- `{}` -> `{tool_name}`", entry.name));
            push_topic_suffix(&mut output, entry);
            output.push('\n');
        }
    }
    if !implicit.is_empty() {
        output.push_str(&format!(
            "\nImplicit entries have no function tool. Call `{LUASKILL_HELP_TOOL_NAME}` with `{{\"name\":\"<canonical name>\"}}` to read the argument schema, then call `{CALL_LUASKILL_TOOL_NAME}` with that name:\n"
        ));
        for entry in implicit {
            output.push_str(&format!("- `{}`", entry.name));
            if !entry.description.is_empty() {
                output.push_str(": ");
                output.push_str(&entry.description);
            }
            push_topic_suffix(&mut output, entry);
            output.push('\n');
        }
    }
    if !package.topics.is_empty() && routes.help_tool_visible {
        output.push_str(&format!(
            "\nHelp topics, read with `{LUASKILL_HELP_TOOL_NAME}` and `{{\"skill\":\"{skill_id}\",\"topic\":\"<topic>\"}}`:\n"
        ));
        push_topic_index(&mut output, package);
    }
    Some(output.trim_end().to_string())
}

/// Renders the `luaskill_help` tool output for one request.
/// 为单个请求渲染 `luaskill_help` 工具输出。
///
/// # Parameters
/// 参数
/// - `request`: Parsed help request.
/// - `request`：已解析的帮助请求。
/// - `packages`: Package help trees visible to the current execution.
/// - `packages`：当前执行可见的包帮助树。
///
/// # Returns
/// 返回值
/// Markdown help text, or an error naming the valid choices.
/// Markdown 帮助文本；失败时返回列出有效选项的错误。
pub fn render_luaskill_help_output(
    request: &LuaSkillHelpRequest,
    packages: &[LuaSkillPackageHelp],
) -> Result<String, String> {
    match request {
        LuaSkillHelpRequest::Package { skill } => {
            render_package_help(find_package(packages, skill)?)
        }
        LuaSkillHelpRequest::Topic { skill, topic } => {
            let package = find_package(packages, skill)?;
            if topic.eq_ignore_ascii_case(LUASKILL_MAIN_HELP_FLOW) {
                return render_package_help(package);
            }
            let Some(found) = package.topic(topic) else {
                let available = package
                    .topics
                    .iter()
                    .map(|topic| format!("`{}`", topic.name))
                    .collect::<Vec<_>>();
                return Err(format!(
                    "LuaSkills package `{}` has no help topic `{topic}`. Available topics: {}.",
                    package.skill_id,
                    if available.is_empty() {
                        String::from("none")
                    } else {
                        available.join(", ")
                    }
                ));
            };
            Ok(format!(
                "# `{}` topic `{}`\n\n{}\n",
                package.skill_id, found.name, found.content
            ))
        }
        LuaSkillHelpRequest::Entry { name } => {
            let Some((package, entry)) = packages.iter().find_map(|package| {
                package
                    .entries
                    .iter()
                    .find(|entry| entry.name.eq_ignore_ascii_case(name))
                    .map(|entry| (package, entry))
            }) else {
                let available = packages
                    .iter()
                    .flat_map(|package| package.entries.iter())
                    .map(|entry| format!("`{}`", entry.name))
                    .collect::<Vec<_>>();
                return Err(format!(
                    "LuaSkills entry `{name}` is not enabled. Enabled entries: {}.",
                    if available.is_empty() {
                        String::from("none")
                    } else {
                        available.join(", ")
                    }
                ));
            };
            render_entry_help(package, entry)
        }
    }
}

/// Renders the main help and topic index of one package.
/// 渲染单个包的主帮助与主题索引。
fn render_package_help(package: &LuaSkillPackageHelp) -> Result<String, String> {
    let mut output = format!("# LuaSkills package `{}`\n\n", package.skill_id);
    if package.main_content.is_empty() {
        output.push_str(&package.description);
        output.push('\n');
    } else {
        output.push_str(&package.main_content);
        output.push('\n');
    }
    if !package.topics.is_empty() {
        output.push_str("\n## Help topics\n\n");
        push_topic_index(&mut output, package);
    }
    Ok(output)
}

/// Renders the help topic of one entry plus its schema when it is implicit.
/// 渲染单个入口的帮助主题；隐式入口另附 Schema。
fn render_entry_help(
    package: &LuaSkillPackageHelp,
    entry: &LuaSkillPromptEntry,
) -> Result<String, String> {
    let mut output = format!("# LuaSkills entry `{}`\n\n", entry.name);
    if !entry.description.is_empty() {
        output.push_str(&entry.description);
        output.push_str("\n\n");
    }
    match entry
        .help_topic
        .as_deref()
        .and_then(|topic| package.topic(topic))
    {
        Some(topic) => {
            output.push_str(&format!("## Help topic `{}`\n\n", topic.name));
            output.push_str(&topic.content);
            output.push('\n');
        }
        None if !package.main_content.is_empty() => {
            output.push_str(&format!("## Package guide `{}`\n\n", package.skill_id));
            output.push_str(&package.main_content);
            output.push('\n');
        }
        None => {}
    }
    if entry.registration == LuaSkillRegistration::Implicit {
        output.push_str(&format!(
            "\n## Arguments for `{CALL_LUASKILL_TOOL_NAME}`\n\n"
        ));
        output.push_str(
            entry
                .schema_markdown
                .as_deref()
                .unwrap_or("This entry takes no arguments."),
        );
        output.push('\n');
    }
    Ok(output)
}

/// Finds one package by skill identifier, ignoring ASCII case.
/// 按技能标识查找单个包，忽略 ASCII 大小写。
fn find_package<'a>(
    packages: &'a [LuaSkillPackageHelp],
    skill: &str,
) -> Result<&'a LuaSkillPackageHelp, String> {
    packages
        .iter()
        .find(|package| package.skill_id.eq_ignore_ascii_case(skill))
        .ok_or_else(|| {
            let available = packages
                .iter()
                .map(|package| format!("`{}`", package.skill_id))
                .collect::<Vec<_>>();
            format!(
                "LuaSkills package `{skill}` has no enabled entries. Enabled packages: {}.",
                if available.is_empty() {
                    String::from("none")
                } else {
                    available.join(", ")
                }
            )
        })
}

/// Appends one bullet per help topic with its description.
/// 为每个帮助主题追加一条带说明的列表项。
fn push_topic_index(output: &mut String, package: &LuaSkillPackageHelp) {
    for topic in &package.topics {
        output.push_str(&format!("- `{}`", topic.name));
        if !topic.description.is_empty() {
            output.push_str(": ");
            output.push_str(&topic.description);
        }
        output.push('\n');
    }
}

/// Appends the help topic of one entry when it has a dedicated topic.
/// 当入口拥有专属主题时追加其帮助主题。
fn push_topic_suffix(output: &mut String, entry: &LuaSkillPromptEntry) {
    if let Some(topic) = entry.help_topic.as_deref() {
        output.push_str(&format!(" (topic `{topic}`)"));
    }
}

/// Renders structured Markdown help from one object-shaped JSON Schema.
/// 从对象形 JSON Schema 渲染结构化 Markdown 说明。
pub fn render_luaskill_schema_markdown(schema: &Value) -> Option<String> {
    let mut output = String::new();
    render_schema_node("input", schema, 0, true, &mut output);
    (!output.trim().is_empty()).then(|| output.trim().to_string())
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
    let mut compact = String::new();
    for word in text.split_whitespace() {
        if !compact.is_empty() {
            compact.push(' ');
        }
        compact.push_str(word);
    }
    compact
}

#[cfg(test)]
mod tests {
    //! Tests for LuaSkills package prompt and help rendering.
    //! LuaSkills 包提示词与帮助渲染测试。

    use super::{
        LUASKILL_HELP_TOOL_NAME, LUASKILL_MAIN_HELP_INLINE_LIMIT_BYTES, LuaSkillHelpRequest,
        LuaSkillHelpTopic, LuaSkillInvocationRoutes, LuaSkillPackageHelp, LuaSkillPromptEntry,
        LuaSkillRegistration, compact_whitespace, parse_luaskill_help_request,
        render_luaskill_help_output, render_luaskill_package_segment,
        render_luaskill_schema_markdown,
    };
    use serde_json::json;
    use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;

    /// Builds one package with a direct entry on topic `rg` and an implicit entry on main help.
    /// 构建一个包：直接入口指向主题 `rg`，隐式入口由主帮助覆盖。
    fn sample_package(main_content: &str) -> LuaSkillPackageHelp {
        LuaSkillPackageHelp::new(
            "pkg",
            "Package summary.",
            main_content,
            vec![LuaSkillHelpTopic {
                name: String::from("rg"),
                description: String::from("Search usage."),
                content: String::from("RG-TOPIC-BODY"),
            }],
            vec![
                LuaSkillPromptEntry::new("pkg-rg", "Search.", LuaSkillRegistration::Direct)
                    .with_help_topic("rg"),
                LuaSkillPromptEntry::new("pkg-hidden", "Hidden.", LuaSkillRegistration::Implicit)
                    .with_help_topic("main")
                    .with_schema_markdown("- `input` (object, required)"),
            ],
        )
    }

    /// Builds routes exposing `pkg-rg` as `pkg_rg` with the dispatcher and help tool visible.
    /// 构建路由：`pkg-rg` 暴露为 `pkg_rg`，调度器与帮助工具可见。
    fn sample_routes() -> LuaSkillInvocationRoutes {
        LuaSkillInvocationRoutes {
            function_tools: [(String::from("pkg-rg"), String::from("pkg_rg"))]
                .into_iter()
                .collect(),
            dispatcher_visible: true,
            help_tool_visible: true,
        }
    }

    /// Verifies the segment inlines small main help and maps every entry to its invocation route.
    /// 验证片段内联较小的主帮助，并将每个入口映射到其调用路由。
    #[test]
    fn package_segment_inlines_main_and_lists_routes() {
        let segment =
            render_luaskill_package_segment(&sample_package("MAIN-BODY"), &sample_routes())
                .expect("segment should render");

        assert!(segment.contains("MAIN-BODY"));
        assert!(segment.contains("`pkg-rg` -> `pkg_rg` (topic `rg`)"));
        assert!(segment.contains("`pkg-hidden`"));
        assert!(segment.contains(CALL_LUASKILL_TOOL_NAME));
        assert!(segment.contains("- `rg`"));
        assert!(!segment.contains("RG-TOPIC-BODY"));
    }

    /// Verifies oversized main help is replaced by its description and a pointer to the help tool.
    /// 验证超限主帮助被替换为其说明与帮助工具指引。
    #[test]
    fn package_segment_defers_oversized_main_help() {
        let oversized = "x".repeat(LUASKILL_MAIN_HELP_INLINE_LIMIT_BYTES + 1);
        let segment =
            render_luaskill_package_segment(&sample_package(&oversized), &sample_routes())
                .expect("segment should render");

        assert!(!segment.contains(&oversized));
        assert!(segment.contains("Package summary."));
        assert!(segment.contains(&format!("`{LUASKILL_HELP_TOOL_NAME}`")));
    }

    /// Verifies entries without a visible route are omitted and fully hidden packages render nothing.
    /// 验证没有可见路由的入口被省略，完全隐藏的包不渲染。
    #[test]
    fn package_segment_omits_unreachable_entries() {
        let package = sample_package("MAIN-BODY");
        let without_dispatcher = LuaSkillInvocationRoutes {
            dispatcher_visible: false,
            ..sample_routes()
        };
        let segment = render_luaskill_package_segment(&package, &without_dispatcher)
            .expect("direct entry keeps the segment");
        assert!(!segment.contains("`pkg-hidden`"));

        let nothing_visible = LuaSkillInvocationRoutes::default();
        assert!(render_luaskill_package_segment(&package, &nothing_visible).is_none());
    }

    /// Verifies help requests require exactly one of `skill` or `name`.
    /// 验证帮助请求必须且只能提供 `skill` 或 `name` 之一。
    #[test]
    fn help_request_parsing_enforces_one_selector() {
        assert_eq!(
            parse_luaskill_help_request(&json!({"skill": "pkg"})),
            Ok(LuaSkillHelpRequest::Package {
                skill: String::from("pkg")
            })
        );
        assert_eq!(
            parse_luaskill_help_request(&json!({"skill": "pkg", "topic": "rg"})),
            Ok(LuaSkillHelpRequest::Topic {
                skill: String::from("pkg"),
                topic: String::from("rg")
            })
        );
        assert_eq!(
            parse_luaskill_help_request(&json!({"name": "pkg-rg"})),
            Ok(LuaSkillHelpRequest::Entry {
                name: String::from("pkg-rg")
            })
        );
        assert!(parse_luaskill_help_request(&json!({})).is_err());
        assert!(parse_luaskill_help_request(&json!({"topic": "rg"})).is_err());
        assert!(parse_luaskill_help_request(&json!({"skill": "pkg", "name": "pkg-rg"})).is_err());
        assert!(parse_luaskill_help_request(&json!({"skill": 1})).is_err());
    }

    /// Verifies each help request mode returns its node and only implicit entries add a schema.
    /// 验证每种帮助请求返回对应节点，且只有隐式入口附带 Schema。
    #[test]
    fn help_output_resolves_package_topic_and_entry() {
        let packages = [sample_package("MAIN-BODY")];
        let render = |args| {
            render_luaskill_help_output(
                &parse_luaskill_help_request(&args).expect("valid request"),
                &packages,
            )
        };

        let package = render(json!({"skill": "PKG"})).expect("package help");
        assert!(package.contains("MAIN-BODY") && package.contains("- `rg`"));
        let topic = render(json!({"skill": "pkg", "topic": "rg"})).expect("topic help");
        assert!(topic.contains("RG-TOPIC-BODY"));
        let direct = render(json!({"name": "pkg-rg"})).expect("direct entry help");
        assert!(direct.contains("RG-TOPIC-BODY") && !direct.contains(CALL_LUASKILL_TOOL_NAME));
        let implicit = render(json!({"name": "pkg-hidden"})).expect("implicit entry help");
        assert!(implicit.contains("MAIN-BODY") && implicit.contains("`input` (object, required)"));

        assert!(render(json!({"skill": "missing"})).is_err());
        assert!(render(json!({"skill": "pkg", "topic": "missing"})).is_err());
        assert!(render(json!({"name": "missing"})).is_err());
    }

    /// Verifies compact help text collapses all whitespace runs into single spaces.
    /// 验证紧凑帮助文本会将所有连续空白折叠为单个空格。
    #[test]
    fn compact_whitespace_collapses_prompt_help_text() {
        assert_eq!(
            compact_whitespace("  Pick\n\tcandidate   files first.  "),
            "Pick candidate files first."
        );
        assert_eq!(compact_whitespace("  \n\t  "), "");
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
