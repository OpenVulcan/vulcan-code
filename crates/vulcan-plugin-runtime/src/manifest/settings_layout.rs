//! Declarative settings layouts rendered by the host page kit instead of plugin-owned modules.
//! 由宿主页面套件渲染、取代插件自带模块的声明式设置版面。

use super::{VulcanPluginActionCaller, VulcanPluginActionEffect, VulcanPluginActionManifest};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Current settings layout format accepted by the host renderer.
/// 宿主渲染器当前接受的设置版面格式版本。
pub const VULCAN_PLUGIN_SETTINGS_LAYOUT_VERSION: u32 = 1;

/// Locale whose messages every layout message key must resolve in, matching the host fallback chain.
/// 每个版面消息键都必须能解析的语言，与宿主回退链一致。
pub const VULCAN_PLUGIN_SETTINGS_FALLBACK_LOCALE: &str = "en";

/// One complete settings page declared as data; the host owns every element and style.
/// 以数据声明的一个完整设置页面；宿主拥有全部元素与样式。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsLayout {
    /// Layout format version; must equal the host-supported version.
    /// 版面格式版本；必须等于宿主支持的版本。
    pub schema_version: u32,
    /// Actions that read and replace the complete configuration object; required when any field exists.
    /// 读取并替换完整配置对象的 Action；存在任一字段时必填。
    #[serde(default)]
    pub config: Option<VulcanPluginSettingsConfigBinding>,
    /// Ordered page sections, each rendered as one host section card.
    /// 有序页面区块，每个渲染为一张宿主区块卡片。
    pub sections: Vec<VulcanPluginSettingsSection>,
}

/// Declared configuration read and write actions.
/// 已声明的配置读取与写入 Action。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsConfigBinding {
    /// Read action returning the complete configuration object.
    /// 返回完整配置对象的读取 Action。
    pub read_action: String,
    /// Mutation action accepting and returning the complete configuration object.
    /// 接收并返回完整配置对象的变更 Action。
    pub write_action: String,
}

/// One titled group of settings items.
/// 一组带标题的设置项。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsSection {
    /// Stable layout-local section identifier.
    /// 稳定的版面内区块标识。
    pub id: String,
    /// Plugin locale message key for the section title.
    /// 区块标题使用的插件语言包消息键。
    pub title_message_key: String,
    /// Plugin locale message key for a one-sentence purpose.
    /// 一句话用途说明使用的插件语言包消息键。
    #[serde(default)]
    pub description_message_key: Option<String>,
    /// Ordered rows of the section.
    /// 区块内的有序行。
    pub items: Vec<VulcanPluginSettingsItem>,
    /// Fixed HTTPS links shown in the section footer and opened through the host URL boundary.
    /// 显示在区块页脚、经宿主地址边界打开的固定 HTTPS 链接。
    #[serde(default)]
    pub links: Vec<VulcanPluginSettingsLink>,
}

/// One outbound documentation or dashboard link.
/// 一个外部文档或控制台链接。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsLink {
    /// Absolute HTTPS URL.
    /// 绝对 HTTPS 地址。
    pub url: String,
    /// Plugin locale message key for the visible link text.
    /// 可见链接文字使用的插件语言包消息键。
    pub label_message_key: String,
}

/// One settings row: a configuration field, a secret credential or a one-shot action.
/// 一个设置行：配置字段、密钥凭据或一次性操作。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginSettingsItem {
    /// Edits one top-level property of the configuration object.
    /// 编辑配置对象的一个顶层属性。
    Field {
        /// Top-level configuration property name.
        /// 顶层配置属性名。
        key: String,
        /// Host control used to edit the property.
        /// 用于编辑属性的宿主控件。
        control: VulcanPluginSettingsControl,
        /// Plugin locale message key for the row label.
        /// 行标签使用的插件语言包消息键。
        label_message_key: String,
        /// Plugin locale message key explaining the effect.
        /// 说明效果的插件语言包消息键。
        #[serde(default)]
        description_message_key: Option<String>,
        /// Plugin locale message key for text and number placeholders.
        /// 文本与数字占位符使用的插件语言包消息键。
        #[serde(default)]
        placeholder_message_key: Option<String>,
        /// Select choices; every value must satisfy the property schema.
        /// 下拉选项；每个值都必须满足属性 Schema。
        #[serde(default)]
        options: Vec<VulcanPluginSettingsOption>,
        /// Host formatting for select options without their own label.
        /// 未提供标签的下拉选项使用的宿主格式。
        #[serde(default)]
        value_format: VulcanPluginSettingsValueFormat,
        /// Checklist choices grouped under headings; every value must satisfy the item schema.
        /// 按标题分组的清单选项；每个值都必须满足元素 Schema。
        #[serde(default)]
        option_groups: Vec<VulcanPluginSettingsOptionGroup>,
        /// Whether checked checklist entries are stored (include) or omitted (exclude).
        /// 清单勾选项是被保存（include）还是被排除（exclude）。
        #[serde(default)]
        selection: VulcanPluginSettingsSelection,
        /// Shows the row only while another field holds one of the listed values.
        /// 仅当另一字段取列出值之一时显示此行。
        #[serde(default)]
        visible_when: Option<VulcanPluginSettingsCondition>,
    },
    /// Stores, replaces or deletes one write-only secret through declared actions.
    /// 通过已声明 Action 保存、替换或删除一个只写密钥。
    Credential {
        /// Stable layout-local item identifier.
        /// 稳定的版面内条目标识。
        id: String,
        /// Plugin locale message key for the row label.
        /// 行标签使用的插件语言包消息键。
        label_message_key: String,
        /// Plugin locale message key explaining the credential.
        /// 说明凭据的插件语言包消息键。
        #[serde(default)]
        description_message_key: Option<String>,
        /// Plugin locale message key for the password placeholder.
        /// 密码占位符使用的插件语言包消息键。
        #[serde(default)]
        placeholder_message_key: Option<String>,
        /// Read action returning at least `configured: boolean`.
        /// 至少返回 `configured: boolean` 的读取 Action。
        status_action: String,
        /// Mutation action accepting an object with the single string `set_argument`.
        /// 接收仅含字符串 `set_argument` 对象的变更 Action。
        set_action: String,
        /// Input property name carrying the secret value.
        /// 承载密钥值的输入属性名。
        set_argument: String,
        /// Optional mutation action removing the stored secret.
        /// 可选的删除已保存密钥的变更 Action。
        #[serde(default)]
        delete_action: Option<String>,
    },
    /// Runs one declared action without arguments, such as a connection test.
    /// 无参数运行一个已声明 Action，例如连接测试。
    Action {
        /// Stable layout-local item identifier.
        /// 稳定的版面内条目标识。
        id: String,
        /// Declared plugin action invoked with an empty object.
        /// 以空对象调用的已声明插件 Action。
        action: String,
        /// Plugin locale message key for the row label and button text.
        /// 行标签与按钮文字使用的插件语言包消息键。
        label_message_key: String,
        /// Plugin locale message key explaining the effect or cost.
        /// 说明效果或代价的插件语言包消息键。
        #[serde(default)]
        description_message_key: Option<String>,
        /// Plugin locale message key shown after success.
        /// 成功后显示的插件语言包消息键。
        #[serde(default)]
        success_message_key: Option<String>,
    },
}

/// Host controls available to declarative fields.
/// 声明式字段可用的宿主控件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginSettingsControl {
    /// Boolean switch.
    /// 布尔开关。
    Switch,
    /// Single choice from explicit options.
    /// 从明确选项中单选。
    Select,
    /// Single-line text.
    /// 单行文本。
    Text,
    /// Numeric entry for integer, number or digit-pattern string properties.
    /// 用于整数、数字或数字模式字符串属性的数值输入。
    Number,
    /// Multiple choice over a string array.
    /// 针对字符串数组的多选清单。
    Checklist,
}

/// Host formatting for option labels.
/// 选项标签的宿主格式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginSettingsValueFormat {
    /// The raw value.
    /// 原始值。
    #[default]
    Plain,
    /// Milliseconds shown as a localized duration.
    /// 毫秒显示为本地化时长。
    DurationMs,
    /// Whole days shown as a localized day count.
    /// 整天数显示为本地化天数。
    Days,
}

/// Checklist storage semantics.
/// 清单存储语义。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginSettingsSelection {
    /// Checked entries are stored in the array.
    /// 勾选项保存在数组中。
    #[default]
    Include,
    /// Unchecked entries are stored in the array.
    /// 未勾选项保存在数组中。
    Exclude,
}

/// One select choice.
/// 一个下拉选项。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsOption {
    /// String or integer value stored in the configuration.
    /// 保存到配置中的字符串或整数值。
    pub value: Value,
    /// Plugin locale message key for the label; omitted labels use `value_format`.
    /// 标签使用的插件语言包消息键；省略时使用 `value_format`。
    #[serde(default)]
    pub label_message_key: Option<String>,
}

/// One titled checklist group.
/// 一个带标题的清单分组。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsOptionGroup {
    /// Stable field-local group identifier.
    /// 稳定的字段内分组标识。
    pub id: String,
    /// Plugin locale message key for the group heading.
    /// 分组标题使用的插件语言包消息键。
    pub title_message_key: String,
    /// String values offered in this group, shown verbatim.
    /// 此分组提供的字符串值，原样显示。
    pub values: Vec<String>,
}

/// Visibility condition on another field.
/// 依赖另一字段的可见条件。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginSettingsCondition {
    /// Configuration property controlled by another field of the same layout.
    /// 由同一版面另一字段控制的配置属性。
    pub key: String,
    /// Values for which the row is visible.
    /// 使此行可见的取值。
    pub values: Vec<Value>,
}

/// One declared action with its resolved schemas, as needed by layout validation.
/// 一个已声明 Action 及其已解析的 Schema，供版面校验使用。
pub(crate) struct SettingsActionContract<'a> {
    pub manifest: &'a VulcanPluginActionManifest,
    pub input: Option<Value>,
    pub output: Option<Value>,
}

/// Inputs to semantic layout validation, all resolved from the same package.
/// 语义版面校验的输入，全部解析自同一插件包。
pub(crate) struct SettingsLayoutContext<'a> {
    pub actions: BTreeMap<&'a str, SettingsActionContract<'a>>,
    pub config_schema: Option<Value>,
    pub messages: &'a BTreeMap<String, String>,
}

/// Validate layout against the package's actions, configuration schema and fallback messages.
/// 依据插件包的 Action、配置 Schema 与回退文案校验版面。
/// mount_id names the owning mount in errors; returns the first concrete violation.
/// mount_id 在错误中命名所属挂载；返回首个具体违规。
pub(crate) fn validate_settings_layout(
    mount_id: &str,
    layout: &VulcanPluginSettingsLayout,
    context: &SettingsLayoutContext<'_>,
) -> Result<(), String> {
    let fail = |message: String| Err(format!("Plugin settings mount `{mount_id}`: {message}"));
    if layout.schema_version != VULCAN_PLUGIN_SETTINGS_LAYOUT_VERSION {
        return fail(format!(
            "schema_version must be {VULCAN_PLUGIN_SETTINGS_LAYOUT_VERSION}."
        ));
    }
    if layout.sections.is_empty() {
        return fail("must declare at least one section.".into());
    }
    let message = |key: &str| -> Result<(), String> {
        if context.messages.get(key).is_some_and(|text| !text.trim().is_empty()) {
            Ok(())
        } else {
            Err(format!(
                "Plugin settings mount `{mount_id}`: message key `{key}` is missing from locale `{VULCAN_PLUGIN_SETTINGS_FALLBACK_LOCALE}`."
            ))
        }
    };
    let optional_message = |key: &Option<String>| key.as_deref().map_or(Ok(()), message);
    // fields maps every configuration key to its property schema so conditions can refer across sections.
    // fields 将每个配置键映射到其属性 Schema，使条件可以跨区块引用。
    let mut fields = BTreeMap::new();
    let mut section_ids = BTreeSet::new();
    let mut item_ids = BTreeSet::new();
    for section in &layout.sections {
        crate::catalog::validate_identifier(&section.id, "settings section id")?;
        if !section_ids.insert(section.id.as_str()) {
            return fail(format!("duplicate section id `{}`.", section.id));
        }
        message(&section.title_message_key)?;
        optional_message(&section.description_message_key)?;
        if section.items.is_empty() {
            return fail(format!("section `{}` must declare at least one item.", section.id));
        }
        for link in &section.links {
            message(&link.label_message_key)?;
            if !link.url.starts_with("https://")
                || link.url.len() <= "https://".len()
                || link.url.chars().any(char::is_whitespace)
            {
                return fail(format!("link `{}` must be an absolute HTTPS URL.", link.url));
            }
        }
        for item in &section.items {
            match item {
                VulcanPluginSettingsItem::Field { key, label_message_key, description_message_key, placeholder_message_key, .. } => {
                    let Some(schema) = context.config_schema.as_ref() else {
                        return fail("fields require a plugin configuration schema.".into());
                    };
                    let property = schema
                        .get("properties")
                        .and_then(|properties| properties.get(key))
                        .ok_or_else(|| format!("Plugin settings mount `{mount_id}`: field `{key}` is not a top-level configuration property."))?;
                    if fields.insert(key.as_str(), property).is_some() {
                        return fail(format!("field `{key}` is declared twice."));
                    }
                    message(label_message_key)?;
                    optional_message(description_message_key)?;
                    optional_message(placeholder_message_key)?;
                }
                VulcanPluginSettingsItem::Credential { id, .. } | VulcanPluginSettingsItem::Action { id, .. } => {
                    crate::catalog::validate_identifier(id, "settings item id")?;
                    if !item_ids.insert(id.as_str()) {
                        return fail(format!("duplicate item id `{id}`."));
                    }
                }
            }
        }
    }
    match (&layout.config, fields.is_empty()) {
        (None, false) => return fail("fields require a `config` binding.".into()),
        (Some(_), true) => return fail("a `config` binding requires at least one field.".into()),
        (Some(binding), false) => {
            require_action(mount_id, context, &binding.read_action, VulcanPluginActionEffect::Read)?;
            require_action(mount_id, context, &binding.write_action, VulcanPluginActionEffect::Mutation)?;
        }
        (None, true) => {}
    }
    for item in layout.sections.iter().flat_map(|section| &section.items) {
        match item {
            VulcanPluginSettingsItem::Field { key, control, options, value_format, option_groups, selection, visible_when, placeholder_message_key, .. } => {
                validate_field(mount_id, key, *control, &FieldShape {
                    property: fields[key.as_str()],
                    options,
                    value_format: *value_format,
                    option_groups,
                    selection: *selection,
                    has_placeholder: placeholder_message_key.is_some(),
                }, &message)?;
                if let Some(condition) = visible_when {
                    let Some(controller) = fields.get(condition.key.as_str()) else {
                        return fail(format!("field `{key}` visibility refers to undeclared field `{}`.", condition.key));
                    };
                    if condition.key == *key || condition.values.is_empty() {
                        return fail(format!("field `{key}` visibility must name another field and at least one value."));
                    }
                    for value in &condition.values {
                        validate_value(mount_id, controller, value, &condition.key)?;
                    }
                }
            }
            VulcanPluginSettingsItem::Credential { label_message_key, description_message_key, placeholder_message_key, status_action, set_action, set_argument, delete_action, .. } => {
                message(label_message_key)?;
                optional_message(description_message_key)?;
                optional_message(placeholder_message_key)?;
                let status = require_action(mount_id, context, status_action, VulcanPluginActionEffect::Read)?;
                if property_type(status.output.as_ref().and_then(|schema| schema.get("properties")).and_then(|properties| properties.get("configured"))) != Some("boolean") {
                    return fail(format!("credential status action `{status_action}` must declare an output schema with boolean `configured`."));
                }
                let set = require_action(mount_id, context, set_action, VulcanPluginActionEffect::Mutation)?;
                let input = set.input.as_ref();
                let single_string = input
                    .and_then(|schema| schema.get("properties"))
                    .and_then(Value::as_object)
                    .is_some_and(|properties| properties.len() == 1 && property_type(properties.get(set_argument)) == Some("string"));
                let required = input
                    .and_then(|schema| schema.get("required"))
                    .and_then(Value::as_array)
                    .is_some_and(|required| required.iter().any(|name| name.as_str() == Some(set_argument)));
                if !single_string || !required {
                    return fail(format!("credential set action `{set_action}` must accept exactly one required string property `{set_argument}`."));
                }
                if let Some(delete) = delete_action {
                    require_empty_input(mount_id, require_action(mount_id, context, delete, VulcanPluginActionEffect::Mutation)?)?;
                }
            }
            VulcanPluginSettingsItem::Action { action, label_message_key, description_message_key, success_message_key, .. } => {
                message(label_message_key)?;
                optional_message(description_message_key)?;
                optional_message(success_message_key)?;
                let contract = context.actions.get(action.as_str()).filter(|contract| contract.manifest.callers.contains(&VulcanPluginActionCaller::PluginUi));
                let Some(contract) = contract else {
                    return fail(format!("action `{action}` must be declared with the `plugin_ui` caller."));
                };
                require_empty_input(mount_id, contract)?;
            }
        }
    }
    Ok(())
}

/// Field properties that shape validation of one control.
/// 影响单个控件校验的字段属性。
struct FieldShape<'a> {
    property: &'a Value,
    options: &'a [VulcanPluginSettingsOption],
    value_format: VulcanPluginSettingsValueFormat,
    option_groups: &'a [VulcanPluginSettingsOptionGroup],
    selection: VulcanPluginSettingsSelection,
    has_placeholder: bool,
}

/// Check that control matches the property type and that its choices satisfy the schema.
/// 检查控件与属性类型匹配，且其选项满足 Schema。
fn validate_field(
    mount_id: &str,
    key: &str,
    control: VulcanPluginSettingsControl,
    shape: &FieldShape<'_>,
    message: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let fail = |text: &str| Err(format!("Plugin settings mount `{mount_id}`: field `{key}` {text}"));
    let kind = property_type(Some(shape.property));
    let compatible = match control {
        VulcanPluginSettingsControl::Switch => kind == Some("boolean"),
        VulcanPluginSettingsControl::Text => kind == Some("string"),
        VulcanPluginSettingsControl::Number => matches!(kind, Some("integer" | "number" | "string")),
        VulcanPluginSettingsControl::Select => matches!(kind, Some("integer" | "string")),
        VulcanPluginSettingsControl::Checklist => {
            kind == Some("array") && property_type(shape.property.get("items")) == Some("string")
        }
    };
    if !compatible {
        return fail("uses a control that does not match its schema type.");
    }
    let is_select = control == VulcanPluginSettingsControl::Select;
    let is_checklist = control == VulcanPluginSettingsControl::Checklist;
    if is_select == shape.options.is_empty() {
        return fail("must declare options exactly when it uses the select control.");
    }
    if !is_select && shape.value_format != VulcanPluginSettingsValueFormat::Plain {
        return fail("may declare value_format only for the select control.");
    }
    if is_checklist == shape.option_groups.is_empty() {
        return fail("must declare option_groups exactly when it uses the checklist control.");
    }
    if !is_checklist && shape.selection != VulcanPluginSettingsSelection::Include {
        return fail("may declare selection only for the checklist control.");
    }
    if shape.has_placeholder && !matches!(control, VulcanPluginSettingsControl::Text | VulcanPluginSettingsControl::Number) {
        return fail("may declare a placeholder only for text and number controls.");
    }
    let mut seen = BTreeSet::new();
    for option in shape.options {
        if !(option.value.is_string() || option.value.is_i64() || option.value.is_u64()) {
            return fail("options must use string or integer values.");
        }
        if !seen.insert(option.value.to_string()) {
            return fail("repeats an option value.");
        }
        validate_value(mount_id, shape.property, &option.value, key)?;
        if let Some(label) = &option.label_message_key {
            message(label)?;
        }
    }
    let mut group_ids = BTreeSet::new();
    let items = shape.property.get("items").unwrap_or(&Value::Null);
    for group in shape.option_groups {
        crate::catalog::validate_identifier(&group.id, "settings option group id")?;
        if !group_ids.insert(group.id.as_str()) || group.values.is_empty() {
            return fail("option groups need distinct ids and at least one value.");
        }
        message(&group.title_message_key)?;
        for value in &group.values {
            if !seen.insert(value.clone()) {
                return fail("repeats a checklist value across groups.");
            }
            validate_value(mount_id, items, &Value::String(value.clone()), key)?;
        }
    }
    Ok(())
}

/// Validate value against one self-contained property schema.
/// 依据一个自包含属性 Schema 校验 value。
fn validate_value(mount_id: &str, property: &Value, value: &Value, key: &str) -> Result<(), String> {
    crate::catalog::validate_json_schema_instance(
        property,
        value,
        &format!("plugin settings mount `{mount_id}` value for `{key}`"),
    )
}

/// Resolve action and require the plugin UI caller and the expected effect.
/// 解析 action，并要求 plugin UI 调用方与预期效果。
fn require_action<'context, 'a>(
    mount_id: &str,
    context: &'context SettingsLayoutContext<'a>,
    action: &str,
    effect: VulcanPluginActionEffect,
) -> Result<&'context SettingsActionContract<'a>, String> {
    context
        .actions
        .get(action)
        .filter(|contract| contract.manifest.effect == effect && contract.manifest.callers.contains(&VulcanPluginActionCaller::PluginUi))
        .ok_or_else(|| format!("Plugin settings mount `{mount_id}`: action `{action}` must be declared with effect `{}` and the `plugin_ui` caller.", match effect {
            VulcanPluginActionEffect::Read => "read",
            VulcanPluginActionEffect::Mutation => "mutation",
        }))
}

/// Require that an action either has no input schema or accepts the empty object.
/// 要求 Action 没有输入 Schema 或接受空对象。
fn require_empty_input(mount_id: &str, contract: &SettingsActionContract<'_>) -> Result<(), String> {
    match &contract.input {
        None => Ok(()),
        Some(schema) => validate_value(mount_id, schema, &Value::Object(Default::default()), &contract.manifest.id),
    }
}

/// Return the single JSON type name of a schema node, rejecting union types.
/// 返回 Schema 节点的单一 JSON 类型名，拒绝联合类型。
fn property_type(schema: Option<&Value>) -> Option<&str> {
    schema?.get("type")?.as_str()
}

#[cfg(test)]
mod tests;
