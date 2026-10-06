// Plugin settings pages: host-rendered `kind: settings` layouts of plugin detail mounts.
// 插件设置页面：由宿主渲染的插件详情挂载 `kind: settings` 版面。
//
// `load_plugin_ui_module` supplies the layout, the configuration schema and the plugin messages;
// every value shown or saved goes through the plugin's declared actions via
// `invoke_plugin_action`, as in the WebUI renderer. Core validated the layout at its supported
// version before serving it, so this renderer reads the declared shape without a second version
// gate. `web` hints are ignored and `tui` hints choose the terminal variant. Module-kind mounts
// are ESM components the terminal cannot run; they are listed with a note.
// `load_plugin_ui_module` 提供版面、配置 Schema 与插件文案；显示或保存的每个值都经
// `invoke_plugin_action` 调用插件声明的 Action，与 WebUI 渲染器一致。Core 在提供版面前已按其支持的
// 版本完成校验，因此本渲染器按声明形状读取，不设第二道版本门槛。忽略 `web` 提示，由 `tui` 提示选择
// 终端变体。模块类挂载是终端无法运行的 ESM 组件，只列出并附说明。

use std::borrow::Cow;
use std::time::UNIX_EPOCH;

use super::*;
use crate::service_api::{CallKind, scrub_secrets, skills_actions as plugins_api};
use crate::ui::text::{compact_count, compact_duration};

/// Mount point whose entries are plugin pages under the plugins group, as in the WebUI.
/// 作为插件分组下插件页面的挂载点，与 WebUI 一致。
const DETAIL_POINT: &str = "skillsPluginDetail";

/// Locale every layout message key resolves in; the protocol value of Core's
/// `VULCAN_PLUGIN_SETTINGS_FALLBACK_LOCALE`, which validation enforces for each layout.
/// 每个版面消息键都能解析的语言；即 Core `VULCAN_PLUGIN_SETTINGS_FALLBACK_LOCALE` 的协议值，校验对
/// 每个版面强制执行。
const FALLBACK_LOCALE: &str = "en";

/// Placeholder in table count messages; the protocol value of Core's
/// `VULCAN_PLUGIN_DATA_COUNT_PLACEHOLDER`.
/// 表格计数文案中的占位符；即 Core `VULCAN_PLUGIN_DATA_COUNT_PLACEHOLDER` 的协议值。
const COUNT_PLACEHOLDER: &str = "{{count}}";

/// Points a chart shows when its layout omits `limit`; the protocol value of Core's
/// `default_chart_limit`, which the served raw layout does not fill in.
/// 版面省略 `limit` 时图表显示的数据点数；即 Core `default_chart_limit` 的协议值，服务提供的原始版面
/// 不会填入该默认值。
const DEFAULT_CHART_LIMIT: u64 = 14;

/// Shown for absent, null or empty values.
/// 缺失、空值或空字符串的显示文本。
const EMPTY: &str = "—";

/// Separates a chart key from a period id inside one choice value.
/// 在一个选项值内分隔图表键与周期标识。
pub const PERIOD_SEPARATOR: char = '\u{1f}';

/// Cells of a table `bar` value at its column maximum.
/// 表格 `bar` 值达到本列最大值时占用的单元格数。
const BAR_CELLS: usize = 10;

/// Runtime-ready plugin detail mounts in catalog order, as `(plugin, mount)` workbench records;
/// mirrors the WebUI's `enabledPluginDetailMounts`.
/// 按目录顺序列出运行就绪的插件详情挂载，形式为 `(插件, 挂载)` 工作台记录；对应 WebUI 的
/// `enabledPluginDetailMounts`。
pub(super) fn detail_mounts(app: &App) -> Vec<(&Value, &Value)> {
    a(app.slots.value(Slot::Workbench), "/plugins")
        .iter()
        .filter(|plugin| {
            b(plugin, "/available")
                && b(plugin, "/enabled")
                && s(plugin, "/runtimeState") == "enabled"
                && v(plugin, "/extension").is_object()
        })
        .flat_map(|plugin| {
            a(plugin, "/extension/uiMounts")
                .iter()
                .filter(|mount| s(mount, "/point") == DETAIL_POINT)
                .map(move |mount| (plugin, mount))
        })
        .collect()
}

/// The open mount while it is still runtime-ready.
/// 仍处于运行就绪状态的当前挂载。
fn selected(app: &App) -> Option<(&Value, &Value)> {
    let (plugin_id, mount_id) = app.plugin_mount.as_ref()?;
    detail_mounts(app)
        .into_iter()
        .find(|(plugin, mount)| s(plugin, "/id") == *plugin_id && s(mount, "/id") == *mount_id)
}

/// Mount navigation label: exact locale, then the first same-language locale, then the default.
/// 挂载导航名称：精确语言，其次第一个同语言语言，最后默认名称。
pub(super) fn mount_label(mount: &Value) -> String {
    let locale = crate::i18n::active_locale();
    let language = locale.split('-').next().unwrap_or(locale);
    let read = |value: &Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let labels = v(mount, "/labelByLocale");
    if let Some(label) = labels.get(locale).and_then(read) {
        return label;
    }
    let mut entries: Vec<(&String, &Value)> = labels
        .as_object()
        .map(|map| map.iter().collect())
        .unwrap_or_default();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries
        .into_iter()
        .find(|(candidate, _)| same_language(candidate, language))
        .and_then(|(_, label)| read(label))
        .unwrap_or_else(|| s(mount, "/label"))
}

/// Whether a locale tag belongs to `language`, e.g. `en` and `en-GB` for `en`.
/// 语言标签是否属于 `language`，例如 `en` 与 `en-GB` 属于 `en`。
fn same_language(candidate: &str, language: &str) -> bool {
    candidate == language
        || candidate
            .strip_prefix(language)
            .is_some_and(|rest| rest.starts_with('-'))
}

/// Title of the open mount's page, or None when no runtime-ready mount is open.
/// 当前挂载页面的标题；未打开运行就绪的挂载时返回 None。
pub(super) fn title(app: &App) -> Option<String> {
    selected(app).map(|(_, mount)| mount_label(mount))
}

/// Navigation rows of the plugin pages, one per runtime-ready detail mount.
/// 插件页面的导航行，每个运行就绪的详情挂载一行。
pub(super) fn navigation(app: &App) -> Vec<super::navigation::NavigationEntry> {
    let open = app.current_page() == Some(PageId::PluginMount);
    detail_mounts(app)
        .into_iter()
        .map(|(plugin, mount)| {
            let (plugin_id, mount_id) = (s(plugin, "/id"), s(mount, "/id"));
            super::navigation::NavigationEntry {
                label: mount_label(mount),
                badge: None,
                selected: open
                    && app.plugin_mount.as_ref() == Some(&(plugin_id.clone(), mount_id.clone())),
                action: UiAction::OpenPluginMount {
                    plugin: plugin_id,
                    mount: mount_id,
                },
            }
        })
        .collect()
}

/// Actions opening a plugin's own pages, for its row on the plugin management page.
/// 打开插件自有页面的动作，用于插件管理页中该插件的行。
pub(super) fn open_actions(app: &App, plugin_id: &str) -> Vec<RowAction> {
    detail_mounts(app)
        .into_iter()
        .filter(|(plugin, _)| s(plugin, "/id") == plugin_id)
        .map(|(_, mount)| {
            action(
                mount_label(mount),
                UiAction::OpenPluginMount {
                    plugin: plugin_id.to_string(),
                    mount: s(mount, "/id"),
                },
                Tone::Accent,
            )
        })
        .collect()
}

/// The loaded settings layout with the identities and catalogs its rows need.
/// 已加载的设置版面，以及其行所需的身份与文案目录。
struct View<'a> {
    /// Plugin owning the mount.
    /// 挂载所属插件。
    plugin: String,
    /// Runtime generation every action call is bound to.
    /// 每次 Action 调用绑定的运行时代际。
    runtime: String,
    /// Parsed layout document.
    /// 已解析的版面文档。
    layout: Value,
    /// Configuration JSON Schema, or null when the plugin declares none.
    /// 配置 JSON Schema；插件未声明时为 null。
    schema: &'a Value,
    /// Plugin messages keyed by locale, then message key.
    /// 按语言、再按消息键组织的插件文案。
    messages: &'a Value,
}

impl View<'_> {
    /// A plugin message: exact locale, same language, then the fallback locale.
    /// 插件文案：精确语言、同语言，最后回退语言。
    fn text(&self, key: &str) -> Option<String> {
        let locale = crate::i18n::active_locale();
        let language = locale.split('-').next().unwrap_or(locale);
        let catalogs = self.messages.as_object()?;
        let same = catalogs
            .iter()
            .find(|(candidate, _)| same_language(candidate, language))
            .map(|(_, messages)| messages);
        [catalogs.get(locale), same, catalogs.get(FALLBACK_LOCALE)]
            .into_iter()
            .flatten()
            .find_map(|messages| {
                messages
                    .get(key)?
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
            })
    }

    /// A plugin message, or its key when the plugin declares none.
    /// 插件文案；插件未声明时返回其键。
    fn message(&self, key: &str) -> String {
        self.text(key).unwrap_or_else(|| key.to_string())
    }

    /// The message named by the key at `pointer` of `record`, when one is declared.
    /// `record` 中 `pointer` 处的键所指文案（若已声明）。
    fn message_at(&self, record: &Value, pointer: &str) -> Option<String> {
        record
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(|key| self.message(key))
    }

    /// One plugin action call bound to this mount's runtime generation.
    /// 绑定到本挂载运行时代际的一次插件 Action 调用。
    fn invoke(&self, action_id: &str, arguments: Value, kind: CallKind) -> ServiceCall {
        plugins_api::invoke_plugin_action(&self.plugin, action_id, arguments, &self.runtime, kind)
    }

    /// Credential items of every section in document order; slot indexes follow this order.
    /// 按文档顺序列出全部区块的凭据条目；槽位下标遵循此顺序。
    fn credentials(&self) -> Vec<&Value> {
        a(&self.layout, "/sections")
            .iter()
            .flat_map(|section| a(section, "/items"))
            .filter(|item| s(item, "/type") == "credential")
            .collect()
    }

    /// Ids of the data sections in document order; slot indexes follow this order.
    /// 按文档顺序列出数据区块的标识；槽位下标遵循此顺序。
    fn data_sections(&self) -> Vec<String> {
        a(&self.layout, "/sections")
            .iter()
            .filter(|section| section.get("data").is_some())
            .map(|section| s(section, "/id"))
            .collect()
    }

    /// The active tab id: the chosen one while it exists, else the first.
    /// 当前标签标识：所选标签仍存在时使用它，否则使用第一个。
    fn active_tab(&self, app: &App) -> Option<String> {
        let tabs = a(&self.layout, "/tabs");
        app.plugin_tab
            .as_ref()
            .filter(|chosen| tabs.iter().any(|tab| s(tab, "/id") == **chosen))
            .cloned()
            .or_else(|| tabs.first().map(|tab| s(tab, "/id")))
    }

    /// Sections shown now: the active tab's sections in tab order, or all without tabs.
    /// 当前显示的区块：有标签时为当前标签按标签顺序列出的区块，否则为全部区块。
    fn shown(&self, app: &App) -> Vec<&Value> {
        let sections = a(&self.layout, "/sections");
        let Some(tab) = self.active_tab(app) else {
            return sections.iter().collect();
        };
        a(&self.layout, "/tabs")
            .iter()
            .find(|candidate| s(candidate, "/id") == tab)
            .map(|tab| {
                a(tab, "/sections")
                    .iter()
                    .filter_map(|id| sections.iter().find(|section| v(section, "/id") == id))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The open mount's settings view, once its module is loaded and is a settings layout.
/// 当前挂载的设置视图；模块已加载且为设置版面时可用。
fn view(app: &App) -> Option<View<'_>> {
    let ui = app.slots.value(Slot::PluginUi);
    if s(ui, "/kind") != "settings" {
        return None;
    }
    Some(View {
        plugin: s(ui, "/pluginId"),
        runtime: s(ui, "/runtimeIdentity"),
        layout: serde_json::from_str(v(ui, "/source").as_str()?).ok()?,
        schema: v(ui, "/configSchema"),
        messages: v(ui, "/messagesByLocale"),
    })
}

/// Slot of the n-th positional item; layouts never approach the slot index range.
/// 第 n 个按位置条目的槽位；版面条目数远小于槽位下标范围。
fn positional(index: usize, slot: fn(u8) -> Slot) -> Option<Slot> {
    u8::try_from(index).ok().map(slot)
}

/// Reads issued when a layout arrives: configuration, credential states and the active tab's
/// data, keeping each data section's chosen arguments.
/// 版面到达时发出的读取：配置、凭据状态与当前标签的数据，并保留各数据区块已选参数。
pub fn reads(app: &App) -> Vec<ServiceCall> {
    let Some(view) = view(app) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    if let Some(read) = v(&view.layout, "/config/read_action").as_str() {
        calls.push(view.invoke(read, json!({}), CallKind::Load(Slot::PluginConfig)));
    }
    for (index, item) in view.credentials().into_iter().enumerate() {
        if let Some(slot) = positional(index, Slot::PluginCredential) {
            calls.push(view.invoke(&s(item, "/status_action"), json!({}), CallKind::Load(slot)));
        }
    }
    calls.extend(data_reads(app, &view, false));
    calls
}

/// Data reads of the active tab that have neither a result nor an error yet.
/// 当前标签中尚无结果也无错误的数据读取。
pub fn tab_reads(app: &App) -> Vec<ServiceCall> {
    view(app).map_or_else(Vec::new, |view| data_reads(app, &view, true))
}

/// Reads of the active tab's data sections; `missing_only` skips settled ones.
/// 当前标签数据区块的读取；`missing_only` 跳过已有结果的区块。
fn data_reads(app: &App, view: &View, missing_only: bool) -> Vec<ServiceCall> {
    let ids = view.data_sections();
    view.shown(app)
        .into_iter()
        .filter_map(|section| {
            let index = ids.iter().position(|id| *id == s(section, "/id"))?;
            let slot = positional(index, Slot::PluginData)?;
            if missing_only && (app.slots.loaded(slot) || app.slot_errors.contains_key(&slot)) {
                return None;
            }
            Some(view.invoke(
                &s(section, "/data/action"),
                data_arguments(app, section, slot),
                CallKind::Load(slot),
            ))
        })
        .collect()
}

/// A data section's current arguments: the last request's, else each declared default.
/// 数据区块当前参数：上次请求的参数，否则为各参数声明的默认值。
fn data_arguments(app: &App, section: &Value, slot: Slot) -> Value {
    if let Some(loader) = app.slots.loader(slot) {
        return loader.param("/arguments").clone();
    }
    Value::Object(
        a(section, "/data/arguments")
            .iter()
            .map(|argument| (s(argument, "/name"), v(argument, "/default").clone()))
            .collect(),
    )
}

/// Whether the scrubbed configuration had secret strings blanked, so a whole-document write
/// would erase them; detected with the same rule that blanks them.
/// 已清理的配置是否有机密字符串被清空，以致整文档写入会将其抹除；使用清空它们的同一规则检测。
fn holds_secret(config: &Value) -> bool {
    fn mark(value: &mut Value) {
        match value {
            Value::String(text) => *text = "\u{1}".into(),
            Value::Array(items) => items.iter_mut().for_each(mark),
            Value::Object(map) => map.values_mut().for_each(mark),
            _ => {}
        }
    }
    let mut marked = config.clone();
    mark(&mut marked);
    let mut scrubbed = marked.clone();
    scrub_secrets(&mut scrubbed);
    scrubbed != marked
}

/// Checks a value against the schema keywords the WebUI checks; `pattern` stays with Core,
/// which re-validates every write.
/// 按 WebUI 检查的 Schema 关键字校验值；`pattern` 交给 Core，Core 会复核每次写入。
/// Returns true when the value cannot be saved.
/// 值不可保存时返回 true。
fn value_issue(schema: &Value, value: &Value) -> bool {
    if !schema.is_object() {
        return false;
    }
    let wrong_type = match s(schema, "/type").as_str() {
        "boolean" => !value.is_boolean(),
        "integer" => !(value.is_i64() || value.is_u64()),
        "number" => !value.is_number(),
        "string" => !value.is_string(),
        "array" => {
            return !value.as_array().is_some_and(|items| {
                items
                    .iter()
                    .all(|item| !value_issue(v(schema, "/items"), item))
            });
        }
        _ => false,
    };
    if wrong_type {
        return true;
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        return true;
    }
    if let Some(number) = value.as_f64() {
        let below = schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| number < min);
        let above = schema
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|max| number > max);
        if below || above {
            return true;
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        let short = schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|min| length < min);
        let long = schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|max| length > max);
        if short || long {
            return true;
        }
    }
    false
}

/// Typed text for a numeric property becomes a number; other text stays text and fails
/// validation, as the WebUI's `numberInputValue`.
/// 数值属性的输入文本转为数字；其他文本保持文本并在校验中失败，与 WebUI 的 `numberInputValue` 一致。
fn number_input(schema: &Value, text: &str) -> Value {
    let text = text.trim();
    if !matches!(s(schema, "/type").as_str(), "integer" | "number") {
        return Value::String(text.to_string());
    }
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    let numeric = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    if !(numeric(whole) && numeric(fraction)) {
        return Value::String(text.to_string());
    }
    text.parse::<i64>()
        .map(Value::from)
        .or_else(|_| text.parse::<f64>().map(Value::from))
        .unwrap_or_else(|_| Value::String(text.to_string()))
}

/// Offered checklist values in group order.
/// 按分组顺序列出清单提供的值。
fn universe(field: &Value) -> Vec<String> {
    a(field, "/option_groups")
        .iter()
        .flat_map(|group| a(group, "/values"))
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// Visibly checked entries of a stored checklist array, by selection mode.
/// 按选择模式求已存清单数组中可见的勾选项。
fn checked_values(field: &Value, stored: &Value) -> Vec<String> {
    let values: Vec<&str> = stored
        .as_array()
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if s(field, "/selection") != "exclude" {
        return values.into_iter().map(str::to_string).collect();
    }
    universe(field)
        .into_iter()
        .filter(|value| !values.contains(&value.as_str()))
        .collect()
}

/// The stored array for a set of checked entries, keeping stored values the layout does not offer.
/// 由勾选项求要保存的数组，并保留版面未提供的已存值。
fn stored_values(field: &Value, stored: &Value, checked: &[String]) -> Value {
    let universe = universe(field);
    let exclude = s(field, "/selection") == "exclude";
    let mut values: Vec<String> = stored
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !universe.iter().any(|offered| offered == value))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    values.extend(
        universe
            .into_iter()
            .filter(|value| checked.contains(value) != exclude),
    );
    values.sort();
    values.dedup();
    json!(values)
}

/// Writes the configuration with one property replaced, after validating it.
/// 校验后写入替换了一个属性的配置。
/// `ctx` carries the mount identities, the write action, the current configuration and the field.
/// `ctx` 携带挂载身份、写入 Action、当前配置与字段。
fn write_property(ctx: &Value, next: Value) -> Option<ServiceCall> {
    if value_issue(v(ctx, "/schema"), &next) {
        return None;
    }
    let mut config = v(ctx, "/config").clone();
    if holds_secret(&config) {
        return None;
    }
    config.as_object_mut()?.insert(s(ctx, "/field/key"), next);
    Some(plugins_api::invoke_plugin_action(
        &s(ctx, "/plugin"),
        &s(ctx, "/write"),
        config,
        &s(ctx, "/runtime"),
        CallKind::Mutate(Cow::Owned(vec![Slot::PluginConfig])),
    ))
}

/// Builds the write for a scalar field edit.
/// 为标量字段编辑构建写入。
fn save_field(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let schema = v(ctx, "/schema");
    let next = match value {
        SettingValue::Bool(on) => Value::Bool(*on),
        SettingValue::Choice(id) => serde_json::from_str(id).ok()?,
        SettingValue::Number(number) => Value::from(*number),
        SettingValue::Text(text) => number_input(schema, text),
    };
    write_property(ctx, next)
}

/// Builds the write for one checklist entry turned on or off.
/// 为单个清单项的开启或关闭构建写入。
fn save_check(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let SettingValue::Bool(on) = value else {
        return None;
    };
    let field = v(ctx, "/field");
    let stored = v(ctx, "/config").get(s(field, "/key"))?;
    let option = s(ctx, "/option");
    let mut checked = checked_values(field, stored);
    checked.retain(|value| *value != option);
    if *on {
        checked.push(option);
    }
    write_property(ctx, stored_values(field, stored, &checked))
}

/// Builds the call storing a typed credential; empty input is refused.
/// 构建保存所输入凭据的调用；空输入被拒绝。
fn save_credential(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let secret = value.as_str();
    if secret.is_empty() {
        return None;
    }
    let slot = positional(n(ctx, "/index") as usize, Slot::PluginCredential)?;
    let mut arguments = serde_json::Map::new();
    arguments.insert(s(ctx, "/argument"), json!(secret));
    Some(plugins_api::invoke_plugin_action(
        &s(ctx, "/plugin"),
        &s(ctx, "/action"),
        Value::Object(arguments),
        &s(ctx, "/runtime"),
        CallKind::Mutate(Cow::Owned(vec![slot])),
    ))
}

/// Builds the read of a data section with one argument changed.
/// 构建改变一个参数后的数据区块读取。
fn load_data(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let slot = positional(n(ctx, "/index") as usize, Slot::PluginData)?;
    let mut arguments = v(ctx, "/arguments").clone();
    arguments
        .as_object_mut()?
        .insert(s(ctx, "/name"), serde_json::from_str(value.as_str()).ok()?);
    Some(plugins_api::invoke_plugin_action(
        &s(ctx, "/plugin"),
        &s(ctx, "/action"),
        arguments,
        &s(ctx, "/runtime"),
        CallKind::Load(slot),
    ))
}

/// An option label: its message, else the value in the field's display format.
/// 选项名称：其文案，否则按字段显示格式渲染的值。
fn option_label(view: &View, option: &Value, format: &str) -> String {
    view.message_at(option, "/label_message_key")
        .unwrap_or_else(|| option_value(v(option, "/value"), format))
}

/// Renders an option value without a label: plain, `duration_ms` or `days`.
/// 渲染未提供名称的选项值：原样、`duration_ms` 或 `days`。
fn option_value(value: &Value, format: &str) -> String {
    match (value.as_i64(), format) {
        (Some(days), "days") => tr!("tui.plugin.days", count = days),
        (Some(ms), "duration_ms") if ms % 1000 == 0 => format!("{} s", ms / 1000),
        (Some(ms), "duration_ms") => format!("{ms} ms"),
        _ => value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_string),
    }
}

/// Choice options whose ids are the JSON encodings of the declared values.
/// 选项标识为声明值 JSON 编码的选项列表。
fn options(view: &View, record: &Value) -> Vec<(String, String)> {
    let format = s(record, "/value_format");
    a(record, "/options")
        .iter()
        .map(|option| {
            (
                v(option, "/value").to_string(),
                option_label(view, option, &format),
            )
        })
        .collect()
}

/// Days since 1970-01-01 as a UTC civil date.
/// 自 1970-01-01 起的天数对应的 UTC 公历日期。
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// A Unix millisecond instant as `YYYY-MM-DD HH:MM UTC`.
/// 将 Unix 毫秒时刻格式化为 `YYYY-MM-DD HH:MM UTC`。
fn utc_minutes(ms: i64) -> String {
    let (year, month, day) = civil(ms.div_euclid(86_400_000));
    let minute = ms.rem_euclid(86_400_000) / 60_000;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minute / 60,
        minute % 60
    )
}

/// Whether text is an ISO calendar date (`YYYY-MM-DD`) or month (`YYYY-MM`).
/// 文本是否为 ISO 日历日期（`YYYY-MM-DD`）或月份（`YYYY-MM`）。
fn calendar(text: &str, month_only: bool) -> bool {
    let shape: &[usize] = if month_only { &[4, 2] } else { &[4, 2, 2] };
    let parts: Vec<&str> = text.split('-').collect();
    parts.len() == shape.len()
        && parts
            .iter()
            .zip(shape)
            .all(|(part, len)| part.len() == *len && part.chars().all(|c| c.is_ascii_digit()))
}

/// Groups the integer digits of a decimal string in threes.
/// 将十进制字符串的整数位按三位分组。
fn grouped(number: f64) -> String {
    let text = if number.fract() == 0.0 {
        format!("{number:.0}")
    } else {
        format!("{number:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    };
    let (sign, rest) = text.split_at(usize::from(text.starts_with('-')));
    let (whole, fraction) = rest.split_once('.').map_or((rest, ""), |(w, f)| (w, f));
    let mut out = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    if fraction.is_empty() {
        format!("{sign}{out}")
    } else {
        format!("{sign}{out}.{fraction}")
    }
}

/// Renders one value in a data format; absent, null or empty values render as `—`.
/// 按数据格式渲染一个值；缺失、空值或空字符串渲染为 `—`。
fn format_value(value: &Value, format: &str) -> String {
    match value {
        Value::Null => return EMPTY.into(),
        Value::String(text) if text.is_empty() => return EMPTY.into(),
        _ => {}
    }
    let raw = || {
        value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_string)
    };
    match format {
        "text" | "code" => return raw(),
        "date" | "month" => {
            return match value.as_str() {
                Some(text) if format == "date" && calendar(text, false) => text[5..].to_string(),
                _ => raw(),
            };
        }
        _ => {}
    }
    let Some(number) = value.as_f64().filter(|number| number.is_finite()) else {
        return raw();
    };
    match format {
        "compact" if number.abs() >= 1000.0 => {
            let sign = if number < 0.0 { "-" } else { "" };
            format!("{sign}{}", compact_count(number.abs() as u64))
        }
        "percent" => format!("{number:.1}%"),
        "ratio" => format!("{number:.1}×"),
        "duration_ms" => compact_duration(number.max(0.0) as u64),
        "timestamp_ms" if number > 0.0 => utc_minutes(number as i64),
        "timestamp_ms" => EMPTY.into(),
        "date_unix_sec" => utc_minutes(number as i64 * 1000)[..10].to_string(),
        _ => grouped(number),
    }
}

/// Reads a dotted path of object keys, as the WebUI's `readPath`; an empty path is the value.
/// 读取由对象键组成的点分路径，与 WebUI 的 `readPath` 一致；空路径即值本身。
fn path<'a>(value: &'a Value, dotted: &str) -> &'a Value {
    if dotted.is_empty() {
        return value;
    }
    dotted
        .split('.')
        .try_fold(value, |current, key| current.as_object()?.get(key))
        .unwrap_or(&Value::Null)
}

/// Object rows of an array path, skipping malformed entries.
/// 数组路径中的对象行，跳过格式错误的条目。
fn rows_at<'a>(value: &'a Value, dotted: &str) -> Vec<&'a Value> {
    path(value, dotted)
        .as_array()
        .map(|items| items.iter().filter(|row| row.is_object()).collect())
        .unwrap_or_default()
}

/// One metric value; ratio metrics divide by a denominator of at least one.
/// 单个指标值；比值指标的分母至少按一计算。
fn metric(source: &Value, item: &Value) -> String {
    if let Some(ratio) = item.get("ratio") {
        let numerator = path(source, &s(ratio, "/numerator")).as_f64();
        let denominator = path(source, &s(ratio, "/denominator")).as_f64();
        return numerator.map_or_else(
            || EMPTY.into(),
            |numerator| {
                format_value(
                    &json!(numerator / denominator.unwrap_or(0.0).max(1.0)),
                    "ratio",
                )
            },
        );
    }
    let format = v(item, "/format").as_str().unwrap_or("number");
    format_value(path(source, &s(item, "/path")), format)
}

/// Builds the rows of the open plugin page.
/// 构建当前插件页面的行。
pub(super) fn rows(app: &App) -> Vec<Row> {
    if selected(app).is_none() {
        return vec![note(tr!("skills.thisPluginPageIs"), Tone::Muted)];
    }
    if let Some(error) = app.slot_errors.get(&Slot::PluginUi) {
        return vec![
            note(tr!("plugins.mount.unableToLoadPlugin"), Tone::Bad),
            note(error.clone(), Tone::Bad),
        ];
    }
    if !app.slots.loaded(Slot::PluginUi) {
        return vec![note(tr!("skills.plugins.loadingPage"), Tone::Muted)];
    }
    let Some(view) = view(app) else {
        return vec![note(tr!("tui.plugin.moduleUnsupported"), Tone::Muted)];
    };
    let mut rows = header(app, &view);
    let tabs = a(&view.layout, "/tabs");
    if !tabs.is_empty() {
        rows.push(choice(
            tr!("plugins.settings.tabs"),
            tabs.iter()
                .map(|tab| (s(tab, "/id"), view.message(&s(tab, "/title_message_key"))))
                .collect(),
            view.active_tab(app).unwrap_or_default(),
            Edit::Local(LocalSetting::PluginTab),
        ));
    }
    for section_record in view.shown(app) {
        rows.push(section(
            view.message(&s(section_record, "/title_message_key")),
        ));
        if let Some(description) = view.message_at(section_record, "/description_message_key") {
            rows.push(note(description, Tone::Muted));
        }
        if section_record.get("data").is_some() {
            rows.extend(data_rows(app, &view, section_record));
        } else {
            rows.extend(item_rows(app, &view, section_record));
        }
        for link in a(section_record, "/links") {
            rows.push(info(
                view.message(&s(link, "/label_message_key")),
                s(link, "/url"),
                Tone::Accent,
            ));
        }
    }
    rows
}

/// Header rows: configuration status, links, document buttons and the open document.
/// 标题区行：配置状态、链接、文档按钮与当前打开的文档。
fn header(app: &App, view: &View) -> Vec<Row> {
    let header = v(&view.layout, "/header");
    let mut rows = Vec::new();
    if let Some(status) = header.get("status") {
        let (text, tone) = if app.slots.loaded(Slot::PluginConfig) {
            (view.message(&s(status, "/ready_message_key")), Tone::Good)
        } else if app.slot_errors.contains_key(&Slot::PluginConfig) {
            (
                view.message(&s(status, "/unavailable_message_key")),
                Tone::Bad,
            )
        } else {
            (tr!("plugins.settings.statusLoading"), Tone::Muted)
        };
        rows.push(note(format!("● {text}"), tone));
    }
    for link in a(header, "/links") {
        rows.push(info(
            view.message(&s(link, "/label_message_key")),
            s(link, "/url"),
            Tone::Accent,
        ));
    }
    for document in a(header, "/documents") {
        rows.push(button(
            view.message(&s(document, "/label_message_key")),
            call(plugins_api::read_plugin_text_asset(
                &view.plugin,
                &s(document, "/asset"),
            )),
            Tone::Normal,
        ));
    }
    // The open document reads in place until it is closed.
    // 打开的文档就地阅读，直到关闭。
    if let Some(loader) = app.slots.loader(Slot::PluginAsset) {
        let asset = loader.param("/path");
        if let Some(document) = a(header, "/documents")
            .iter()
            .find(|document| v(document, "/asset") == asset)
        {
            rows.push(section(view.message(&s(document, "/title_message_key"))));
            rows.push(Row::new(
                RowKind::Document {
                    content: s(app.slots.value(Slot::PluginAsset), "/content"),
                },
                "",
            ));
            rows.push(button(
                tr!("tui.plugin.closeDocument"),
                UiAction::ClearSlot(Slot::PluginAsset),
                Tone::Muted,
            ));
        }
    }
    if let Some(error) = app.slot_errors.get(&Slot::PluginAsset) {
        rows.push(note(error.clone(), Tone::Bad));
    }
    rows
}

/// Rows of a settings section: fields, credentials and actions.
/// 设置区块的行：字段、凭据与操作。
fn item_rows(app: &App, view: &View, section_record: &Value) -> Vec<Row> {
    let mut rows = Vec::new();
    let items = a(section_record, "/items");
    let has_fields = items.iter().any(|item| s(item, "/type") == "field");
    let config = app
        .slots
        .loaded(Slot::PluginConfig)
        .then(|| app.slots.value(Slot::PluginConfig));
    // Fields edit only a loaded configuration that holds no secret this client never receives.
    // 字段只编辑已加载、且不含本客户端从不接收的机密的配置。
    let editable = config.filter(|config| !holds_secret(config));
    if has_fields {
        match config {
            None if app.slot_errors.contains_key(&Slot::PluginConfig) => {
                rows.push(note(tr!("plugins.settings.loadFailed"), Tone::Bad));
                if let Some(error) = app.slot_errors.get(&Slot::PluginConfig) {
                    rows.push(note(error.clone(), Tone::Bad));
                }
            }
            None => rows.push(note(tr!("tui.page.loading"), Tone::Muted)),
            Some(_) if editable.is_none() => {
                rows.push(note(tr!("tui.plugin.secretConfig"), Tone::Warn));
            }
            Some(_) => {}
        }
    }
    let credentials = view.credentials();
    for item in items {
        match s(item, "/type").as_str() {
            "field" => {
                if let Some(config) = config {
                    rows.extend(field_rows(view, item, config, editable.is_some()));
                }
            }
            "credential" => {
                if let Some(index) = credentials
                    .iter()
                    .position(|candidate| std::ptr::eq(*candidate, item))
                {
                    rows.extend(credential_rows(app, view, item, index));
                }
            }
            "action" => {
                let reload: Vec<Slot> = (0..credentials.len())
                    .filter_map(|index| positional(index, Slot::PluginCredential))
                    .collect();
                if let Some(description) = view.message_at(item, "/description_message_key") {
                    rows.push(note(description, Tone::Muted));
                }
                rows.push(button(
                    view.message(&s(item, "/label_message_key")),
                    call(view.invoke(
                        &s(item, "/action"),
                        json!({}),
                        CallKind::Mutate(Cow::Owned(reload)),
                    )),
                    Tone::Accent,
                ));
            }
            _ => {}
        }
    }
    rows
}

/// Rows of one configuration field, hidden while its `visible_when` condition is unmet.
/// 单个配置字段的行；`visible_when` 条件不满足时隐藏。
fn field_rows(view: &View, field: &Value, config: &Value, editable: bool) -> Vec<Row> {
    if let Some(condition) = field.get("visible_when")
        && !a(condition, "/values").contains(v(config, &format!("/{}", s(condition, "/key"))))
    {
        return Vec::new();
    }
    let key = s(field, "/key");
    let label = view.message(&s(field, "/label_message_key"));
    let description = view
        .message_at(field, "/description_message_key")
        .unwrap_or_default();
    let schema = view
        .schema
        .pointer(&format!("/properties/{key}"))
        .unwrap_or(&Value::Null);
    let current = config.get(&key).unwrap_or(&Value::Null);
    let control = s(field, "/control");
    let format = s(field, "/value_format");
    if !editable {
        let shown = match control.as_str() {
            "select" => a(field, "/options")
                .iter()
                .find(|option| v(option, "/value") == current)
                .map_or_else(
                    || format_value(current, "text"),
                    |option| option_label(view, option, &format),
                ),
            "checklist" => checked_values(field, current).join(", "),
            _ => format_value(current, "text"),
        };
        return vec![info(label, shown, Tone::Muted).detail(description)];
    }
    let ctx = json!({
        "plugin": view.plugin,
        "runtime": view.runtime,
        "write": s(&view.layout, "/config/write_action"),
        "config": config,
        "field": field,
        "schema": schema,
    });
    let edit = Edit::Call(Setter::Build {
        ctx: ctx.clone(),
        build: save_field,
    });
    let row = match control.as_str() {
        "switch" => toggle(label, current == &Value::Bool(true), edit),
        "select" => choice(label, options(view, field), current.to_string(), edit),
        "number"
            if s(schema, "/type") == "integer"
                && schema.get("minimum").is_some_and(Value::is_i64)
                && schema.get("maximum").is_some_and(Value::is_i64) =>
        {
            number(
                label,
                current.as_i64().unwrap_or(n(schema, "/minimum")),
                (n(schema, "/minimum"), n(schema, "/maximum"), 1),
                edit,
            )
        }
        "checklist" => return checklist_rows(view, field, current, label, description, &ctx),
        _ => text(
            label,
            current
                .as_str()
                .map_or_else(|| current.to_string(), str::to_string),
            edit,
        ),
    };
    vec![row.detail(description)]
}

/// Rows of a checklist field: a summary with select-all and clear, then one switch per entry
/// under its group title.
/// 清单字段的行：带全选与清空的摘要，以及各分组标题下每个条目一个开关。
fn checklist_rows(
    view: &View,
    field: &Value,
    stored: &Value,
    label: String,
    description: String,
    ctx: &Value,
) -> Vec<Row> {
    let checked = checked_values(field, stored);
    let all = universe(field);
    let selected_count = all.iter().filter(|value| checked.contains(value)).count();
    let write =
        |values: Vec<String>| write_property(ctx, stored_values(field, stored, &values)).map(call);
    let actions = [
        (tr!("plugins.settings.selectAll"), write(all.clone())),
        (tr!("plugins.settings.clear"), write(Vec::new())),
    ]
    .into_iter()
    .filter_map(|(text, perform)| Some(action(text, perform?, Tone::Normal)))
    .collect();
    let mut rows = vec![
        item(
            &format!("checklist:{}", s(field, "/key")),
            label,
            tr!(
                "plugins.settings.selectedCount",
                selected = selected_count,
                total = all.len()
            ),
        )
        .actions(actions),
    ];
    if !description.is_empty() {
        rows.push(note(description, Tone::Muted));
    }
    for group in a(field, "/option_groups") {
        rows.push(note(
            view.message(&s(group, "/title_message_key")),
            Tone::Accent,
        ));
        for value in a(group, "/values").iter().filter_map(Value::as_str) {
            let entry_label = a(field, "/options")
                .iter()
                .find(|option| v(option, "/value").as_str() == Some(value))
                .and_then(|option| view.message_at(option, "/label_message_key"))
                .unwrap_or_else(|| value.to_string());
            let mut entry = ctx.clone();
            entry["option"] = json!(value);
            rows.push(toggle(
                entry_label,
                checked.iter().any(|candidate| candidate == value),
                Edit::Call(Setter::Build {
                    ctx: entry,
                    build: save_check,
                }),
            ));
        }
    }
    rows
}

/// A credential item: its state, a masked one-shot input and, when configured, deletion.
/// 凭据条目：其状态、遮蔽的一次性输入，以及已配置时的删除。
fn credential_rows(app: &App, view: &View, item: &Value, index: usize) -> Vec<Row> {
    let Some(slot) = positional(index, Slot::PluginCredential) else {
        return Vec::new();
    };
    let label = view.message(&s(item, "/label_message_key"));
    let status = app.slots.value(slot);
    let configured = b(status, "/configured");
    let (state, tone) = if let Some(error) = app.slot_errors.get(&slot) {
        (error.clone(), Tone::Bad)
    } else if !app.slots.loaded(slot) {
        (tr!("plugins.settings.statusLoading"), Tone::Muted)
    } else if !configured {
        (tr!("plugins.settings.credentialMissing"), Tone::Warn)
    } else {
        match s(status, "/validationStatus").as_str() {
            "valid" => (tr!("plugins.settings.credentialValid"), Tone::Good),
            "invalid" => (tr!("plugins.settings.credentialInvalid"), Tone::Bad),
            "untested" => (tr!("plugins.settings.credentialUntested"), Tone::Warn),
            _ => (tr!("plugins.settings.credentialConfigured"), Tone::Good),
        }
    };
    let set = Edit::Call(Setter::Build {
        ctx: json!({
            "plugin": view.plugin,
            "runtime": view.runtime,
            "action": s(item, "/set_action"),
            "argument": s(item, "/set_argument"),
            "index": index,
        }),
        build: save_credential,
    });
    let hint = view
        .message_at(item, "/placeholder_message_key")
        .unwrap_or_default();
    let mut actions = vec![action(
        if configured {
            tr!("plugins.settings.credentialReplace")
        } else {
            tr!("plugins.settings.credentialSave")
        },
        UiAction::Prompt(Box::new(PromptDialog::new(
            label.clone(),
            hint,
            String::new(),
            PromptThen::Edit(set),
            true,
        ))),
        Tone::Accent,
    )];
    if configured && let Some(delete) = v(item, "/delete_action").as_str() {
        actions.push(action(
            tr!("plugins.settings.credentialDelete"),
            confirm(
                tr!("plugins.settings.credentialConfirmDelete"),
                label.clone(),
                tr!("plugins.settings.credentialDelete"),
                call(view.invoke(delete, json!({}), CallKind::Mutate(Cow::Owned(vec![slot])))),
            ),
            Tone::Bad,
        ));
    }
    let description = view
        .message_at(item, "/description_message_key")
        .unwrap_or_default();
    vec![
        super::item(
            &format!("credential:{}", s(item, "/id")),
            label,
            description,
        )
        .badge(state, tone)
        .actions(actions),
    ]
}

/// Rows of a data section: argument choices, refresh, state notes and its blocks.
/// 数据区块的行：参数选项、刷新、状态说明与其数据块。
fn data_rows(app: &App, view: &View, section_record: &Value) -> Vec<Row> {
    let id = s(section_record, "/id");
    let Some(index) = view
        .data_sections()
        .iter()
        .position(|candidate| *candidate == id)
        .and_then(|index| u8::try_from(index).ok())
    else {
        return Vec::new();
    };
    let slot = Slot::PluginData(index);
    let source = v(section_record, "/data");
    let arguments = data_arguments(app, section_record, slot);
    let mut rows = Vec::new();
    for argument in a(source, "/arguments") {
        let name = s(argument, "/name");
        rows.push(choice(
            view.message(&s(argument, "/label_message_key")),
            options(view, argument),
            v(&arguments, &format!("/{name}")).to_string(),
            Edit::Call(Setter::Build {
                ctx: json!({
                    "plugin": view.plugin,
                    "runtime": view.runtime,
                    "action": s(source, "/action"),
                    "arguments": arguments,
                    "name": name,
                    "index": index,
                }),
                build: load_data,
            }),
        ));
    }
    let reload = view.invoke(
        &s(source, "/action"),
        arguments.clone(),
        CallKind::Load(slot),
    );
    if b(source, "/refreshable") {
        rows.push(button(
            tr!("plugins.data.refresh"),
            call(reload.clone()),
            Tone::Muted,
        ));
    }
    if b(source, "/show_loaded_at") {
        let loaded = app.plugin_loaded_at.get(&index).map_or_else(
            || tr!("plugins.data.never"),
            |at| {
                let ms = at
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_millis() as i64);
                utc_minutes(ms)
            },
        );
        rows.push(info(tr!("plugins.data.lastLoaded"), loaded, Tone::Muted));
    }
    if let Some(error) = app.slot_errors.get(&slot) {
        rows.push(note(tr!("plugins.data.loadFailed"), Tone::Bad));
        rows.push(note(error.clone(), Tone::Bad));
        rows.push(button(
            tr!("plugins.mount.retry"),
            call(reload),
            Tone::Normal,
        ));
        return rows;
    }
    if !app.slots.loaded(slot) {
        rows.push(note(tr!("plugins.data.loading"), Tone::Muted));
        return rows;
    }
    let result = app.slots.value(slot);
    if let Some(available) = source.get("available_path").and_then(Value::as_str)
        && path(result, available) != &Value::Bool(true)
    {
        rows.push(note(
            view.message_at(source, "/unavailable_message_key")
                .unwrap_or_default(),
            Tone::Warn,
        ));
        if let Some(reason) = source
            .get("error_path")
            .and_then(Value::as_str)
            .and_then(|error| path(result, error).as_str())
            .filter(|reason| !reason.is_empty())
        {
            rows.push(note(reason.to_string(), Tone::Muted));
        }
        return rows;
    }
    for (position, block) in a(section_record, "/blocks").iter().enumerate() {
        if b(block, "/tui/hidden") {
            continue;
        }
        let key = format!("{id}/{position}");
        rows.extend(match s(block, "/type").as_str() {
            "metrics" => metrics_rows(view, block, result),
            "chart" => chart_rows(app, view, block, result, &key),
            "table" => table_rows(view, block, result),
            _ => Vec::new(),
        });
    }
    rows
}

/// A block's title row, when it declares one.
/// 数据块声明标题时的标题行。
fn block_title(view: &View, block: &Value, trailing: Option<String>) -> Option<Row> {
    let title = view.message_at(block, "/title_message_key")?;
    Some(note(
        trailing.map_or_else(|| title.clone(), |note| format!("{title} · {note}")),
        Tone::Accent,
    ))
}

/// Metrics as a strip of figure tiles.
/// 以数值卡片条显示指标。
fn metrics_rows(view: &View, block: &Value, result: &Value) -> Vec<Row> {
    let mut rows: Vec<Row> = block_title(view, block, None).into_iter().collect();
    let source = match block.get("source_path").and_then(Value::as_str) {
        Some(dotted) => path(result, dotted),
        None => result,
    };
    if block.get("source_path").is_some() && !source.is_object() {
        rows.push(note(
            view.message_at(block, "/empty_message_key")
                .unwrap_or_default(),
            Tone::Muted,
        ));
        return rows;
    }
    rows.push(stats(
        a(block, "/items")
            .iter()
            .map(|item| {
                (
                    view.message(&s(item, "/label_message_key")),
                    metric(source, item),
                    Tone::Normal,
                )
            })
            .collect(),
    ));
    rows
}

/// A chart as a sparkline of its latest points, or as one row per point with the `table`
/// variant; several periods add a period choice.
/// 以最近数据点的迷你走势图显示图表，`table` 变体则每个数据点一行；多个周期时增加周期选项。
fn chart_rows(app: &App, view: &View, block: &Value, result: &Value, key: &str) -> Vec<Row> {
    let mut rows: Vec<Row> = block_title(view, block, None).into_iter().collect();
    let periods = a(block, "/periods");
    let Some(period) = app
        .plugin_periods
        .get(key)
        .and_then(|chosen| periods.iter().find(|period| s(period, "/id") == *chosen))
        .or_else(|| periods.first())
    else {
        return rows;
    };
    if periods.len() > 1 {
        rows.push(choice(
            tr!("plugins.data.periods"),
            periods
                .iter()
                .map(|candidate| {
                    (
                        format!("{key}{PERIOD_SEPARATOR}{}", s(candidate, "/id")),
                        view.message(&s(candidate, "/label_message_key")),
                    )
                })
                .collect(),
            format!("{key}{PERIOD_SEPARATOR}{}", s(period, "/id")),
            Edit::Local(LocalSetting::PluginChartPeriod),
        ));
    }
    let value_path = s(block, "/value_path");
    let value_format = v(block, "/value_format").as_str().unwrap_or("number");
    let label_format = v(period, "/label_format").as_str().unwrap_or("text");
    let mut points = rows_at(result, &s(period, "/path"));
    // Only the latest `limit` points are shown, as in the WebUI.
    // 与 WebUI 一致，只显示最近 `limit` 个数据点。
    let limit = block
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CHART_LIMIT);
    let skip = points.len().saturating_sub(limit as usize);
    points.drain(..skip);
    let value_label = view.message(&s(block, "/value_label_message_key"));
    if points.is_empty() {
        rows.push(note(
            view.message(&s(block, "/empty_message_key")),
            Tone::Muted,
        ));
        return rows;
    }
    let point_label = |row: &Value| {
        a(period, "/label_paths")
            .iter()
            .filter_map(Value::as_str)
            .map(|dotted| format_value(path(row, dotted), label_format))
            .collect::<Vec<_>>()
            .join(" – ")
    };
    if s(block, "/tui/variant") == "table" {
        rows.push(note(value_label, Tone::Muted));
        for row in points {
            let mut value = format_value(path(row, &value_path), value_format);
            if let Some(detail) = block.get("detail") {
                let format = v(detail, "/format").as_str().unwrap_or("text");
                value = format!(
                    "{value} · {}",
                    format_value(path(row, &s(detail, "/path")), format)
                );
            }
            rows.push(info(point_label(row), value, Tone::Normal));
        }
        return rows;
    }
    let latest = points[points.len() - 1];
    rows.push(Row::new(
        RowKind::Spark {
            points: points
                .iter()
                .map(|row| path(row, &value_path).as_f64().unwrap_or(0.0))
                .collect(),
            text: format!(
                "{} {}",
                point_label(latest),
                format_value(path(latest, &value_path), value_format)
            ),
        },
        value_label,
    ));
    rows
}

/// A table as one row per entry: rank and first column, then the other columns.
/// 以每个条目一行显示表格：名次与第一列，其后为其余列。
fn table_rows(view: &View, block: &Value, result: &Value) -> Vec<Row> {
    let entries = rows_at(result, &s(block, "/path"));
    let count = view
        .message_at(block, "/count_message_key")
        .filter(|_| !entries.is_empty())
        .map(|template| template.replace(COUNT_PLACEHOLDER, &entries.len().to_string()));
    let mut rows: Vec<Row> = block_title(view, block, count).into_iter().collect();
    if entries.is_empty() {
        rows.push(note(
            view.message(&s(block, "/empty_message_key")),
            Tone::Muted,
        ));
        return rows;
    }
    let columns = a(block, "/columns");
    let maxima: Vec<f64> = columns
        .iter()
        .map(|column| {
            entries
                .iter()
                .filter_map(|row| path(row, &s(column, "/path")).as_f64())
                .fold(1.0, f64::max)
        })
        .collect();
    let cell = |row: &Value, column: &Value, max: f64| {
        let value = path(row, &s(column, "/path"));
        match v(column, "/format").as_str().unwrap_or("text") {
            "bar" => {
                let ratio = value.as_f64().unwrap_or(0.0) / max;
                "█".repeat(((ratio * BAR_CELLS as f64).round() as usize).clamp(1, BAR_CELLS))
            }
            format => format_value(value, format),
        }
    };
    for (rank, row) in entries.iter().enumerate() {
        let mut cells = columns.iter().zip(&maxima);
        let first = cells
            .next()
            .map(|(column, max)| cell(row, column, *max))
            .unwrap_or_default();
        let label = if b(block, "/rank") {
            format!("{}. {first}", rank + 1)
        } else {
            first
        };
        let detail = cells
            .map(|(column, max)| {
                format!(
                    "{}: {}",
                    view.message(&s(column, "/label_message_key")),
                    cell(row, column, *max)
                )
            })
            .collect::<Vec<_>>()
            .join("  ·  ");
        rows.push(super::item(
            &format!("{}:{rank}", s(block, "/path")),
            label,
            detail,
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checklist storage keeps unoffered values and honours the exclude mode.
    /// 清单存储保留未提供的值，并遵循排除模式。
    #[test]
    fn checklist_storage_round_trips_both_selection_modes() {
        let field = json!({
            "key": "routes",
            "option_groups": [{ "id": "g", "title_message_key": "t", "values": ["a", "b", "c"] }],
        });
        let stored = json!(["a", "zz"]);
        assert_eq!(
            checked_values(&field, &stored),
            vec!["a".to_string(), "zz".to_string()]
        );
        let next = stored_values(&field, &stored, &["a".into(), "c".into()]);
        assert_eq!(next, json!(["a", "c", "zz"]));

        let exclude = json!({
            "key": "routes",
            "selection": "exclude",
            "option_groups": [{ "id": "g", "title_message_key": "t", "values": ["a", "b", "c"] }],
        });
        assert_eq!(
            checked_values(&exclude, &json!(["b"])),
            vec!["a".to_string(), "c".to_string()]
        );
        assert_eq!(
            stored_values(&exclude, &json!(["b"]), &["a".into()]),
            json!(["b", "c"])
        );
    }

    /// Schema checks reject wrong types, enums, ranges and lengths before a write is sent.
    /// Schema 检查在发送写入前拒绝错误类型、枚举、范围与长度。
    #[test]
    fn value_issue_checks_the_supported_keywords() {
        let integer = json!({ "type": "integer", "minimum": 1, "maximum": 10 });
        assert!(!value_issue(&integer, &json!(5)));
        assert!(value_issue(&integer, &json!(11)));
        assert!(value_issue(&integer, &json!("5")));
        assert!(value_issue(&json!({ "enum": ["a"] }), &json!("b")));
        assert!(value_issue(
            &json!({ "type": "string", "maxLength": 2 }),
            &json!("abc")
        ));
        assert!(value_issue(
            &json!({ "type": "array", "items": { "type": "string" } }),
            &json!(["a", 1])
        ));
        assert!(!value_issue(&Value::Null, &json!("anything")));
        assert_eq!(number_input(&integer, " 7 "), json!(7));
        assert_eq!(number_input(&integer, "7x"), json!("7x"));
    }

    /// Secret-named strings blanked by scrubbing make a whole-configuration write unsafe.
    /// 被清理清空的机密命名字符串使整份配置写入不安全。
    #[test]
    fn configurations_with_scrubbed_secrets_are_not_rewritten() {
        assert!(holds_secret(&json!({ "apiKey": "", "fast": true })));
        assert!(holds_secret(&json!({ "nested": { "accessToken": "" } })));
        assert!(!holds_secret(
            &json!({ "fast": true, "mode": "selected", "routes": ["a"] })
        ));
    }

    /// Data formats render the WebUI's display shapes, with `—` for absent values.
    /// 数据格式渲染 WebUI 的显示形状，缺失值显示 `—`。
    #[test]
    fn data_formats_render_display_shapes() {
        assert_eq!(format_value(&Value::Null, "number"), EMPTY);
        assert_eq!(format_value(&json!(1234567), "number"), "1,234,567");
        assert_eq!(format_value(&json!(-1234.5), "number"), "-1,234.5");
        assert_eq!(format_value(&json!(12.34), "percent"), "12.3%");
        assert_eq!(format_value(&json!(2.5), "ratio"), "2.5×");
        assert_eq!(format_value(&json!("2026-10-06"), "date"), "10-06");
        assert_eq!(
            format_value(&json!(1_791_244_800_000_i64), "timestamp_ms"),
            "2026-10-06 00:00 UTC"
        );
        assert_eq!(
            format_value(&json!(1_791_244_800), "date_unix_sec"),
            "2026-10-06"
        );
        assert_eq!(
            metric(
                &json!({ "a": 9, "b": 0 }),
                &json!({ "ratio": { "numerator": "a", "denominator": "b" } })
            ),
            "9.0×"
        );
    }
}
