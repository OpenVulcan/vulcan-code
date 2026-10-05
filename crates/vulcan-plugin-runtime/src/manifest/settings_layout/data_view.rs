//! Read-only data views of a settings layout: one read action rendered as metrics, a period chart and tables.
//! 设置版面的只读数据视图：把一个读取 Action 渲染为指标、周期图表与表格。
//!
//! Blocks describe what to show, not how; each surface (WebUI, TUI) owns its rendering and reads only its own hints.
//! 数据块描述显示什么而非如何显示；各界面（WebUI、TUI）自行渲染，只读取属于自己的提示。

use super::{
    SettingsLayoutContext, VulcanPluginSettingsOption, VulcanPluginSettingsValueFormat,
    validate_value,
};
use crate::manifest::{VulcanPluginActionCaller, VulcanPluginActionEffect};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// Largest number of most-recent points a chart may declare.
/// 图表可声明的最近数据点数量上限。
pub const VULCAN_PLUGIN_DATA_CHART_MAX_POINTS: u32 = 60;

/// Largest metric column count a WebUI hint may request.
/// WebUI 提示可请求的指标列数上限。
pub const VULCAN_PLUGIN_DATA_MAX_METRIC_COLUMNS: u8 = 6;

/// Placeholder a table count message must contain; hosts replace it with the formatted row count.
/// 表格计数文案必须包含的占位符；宿主将其替换为格式化后的行数。
pub const VULCAN_PLUGIN_DATA_COUNT_PLACEHOLDER: &str = "{{count}}";

/// One read action whose result the section's blocks display.
/// 一个读取 Action，其结果由区块的数据块显示。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataSource {
    /// Declared read action with the plugin UI caller.
    /// 带 plugin UI 调用方的已声明读取 Action。
    pub action: String,
    /// User-selectable action arguments shown in the section header; together they form the complete input object.
    /// 显示在区块标题栏、可由用户选择的 Action 参数；合起来构成完整输入对象。
    #[serde(default)]
    pub arguments: Vec<VulcanPluginDataArgument>,
    /// Shows a refresh button that bypasses the action's declared result cache.
    /// 显示一个绕过 Action 声明结果缓存的刷新按钮。
    #[serde(default)]
    pub refreshable: bool,
    /// Shows when this page last received the result.
    /// 显示本页最近一次收到结果的时间。
    #[serde(default)]
    pub show_loaded_at: bool,
    /// Boolean result path; false shows the unavailable state instead of the blocks.
    /// 布尔结果路径；为 false 时显示不可用状态而非数据块。
    #[serde(default)]
    pub available_path: Option<String>,
    /// String result path holding the reason shown in the unavailable state.
    /// 保存不可用原因的字符串结果路径。
    #[serde(default)]
    pub error_path: Option<String>,
    /// Plugin locale message key titling the unavailable state; required with `available_path`.
    /// 不可用状态标题使用的插件语言包消息键；声明 `available_path` 时必填。
    #[serde(default)]
    pub unavailable_message_key: Option<String>,
    /// Current-plugin invalidation topics that reload the data.
    /// 触发重新读取数据的当前插件失效主题。
    #[serde(default)]
    pub refresh_topics: Vec<String>,
}

/// One action input property chosen from fixed options.
/// 一个从固定选项中选择的 Action 输入属性。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataArgument {
    /// Top-level input property name.
    /// 顶层输入属性名。
    pub name: String,
    /// Plugin locale message key for the accessible control label.
    /// 控件无障碍标签使用的插件语言包消息键。
    pub label_message_key: String,
    /// Offered values; each must satisfy the input schema.
    /// 提供的取值；每个都必须满足输入 Schema。
    pub options: Vec<VulcanPluginSettingsOption>,
    /// Host formatting for options without their own label.
    /// 未提供标签的选项使用的宿主格式。
    #[serde(default)]
    pub value_format: VulcanPluginSettingsValueFormat,
    /// Initially selected option value.
    /// 初始选中的选项值。
    pub default: Value,
}

/// One block of a data section.
/// 数据区块中的一个数据块。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginDataBlock {
    /// A group of key figures.
    /// 一组关键数字。
    Metrics {
        /// Plugin locale message key titling the group.
        /// 分组标题使用的插件语言包消息键。
        #[serde(default)]
        title_message_key: Option<String>,
        /// Plugin locale message key for secondary help next to the title; requires a title.
        /// 标题旁次要帮助使用的插件语言包消息键；需要标题。
        #[serde(default)]
        help_message_key: Option<String>,
        /// Object path the metric paths are relative to; when it is null or absent the empty message shows.
        /// 指标路径的相对对象路径；为 null 或缺失时显示空状态文案。
        #[serde(default)]
        source_path: Option<String>,
        /// Plugin locale message key shown while `source_path` holds no object; required with it.
        /// `source_path` 没有对象时显示的插件语言包消息键；与其同时声明。
        #[serde(default)]
        empty_message_key: Option<String>,
        /// Ordered figures.
        /// 有序数字。
        items: Vec<VulcanPluginDataMetric>,
        /// WebUI presentation hints.
        /// WebUI 呈现提示。
        #[serde(default)]
        web: VulcanPluginDataWebHints,
        /// Terminal UI presentation hints.
        /// 终端界面呈现提示。
        #[serde(default)]
        tui: VulcanPluginDataTuiHints,
    },
    /// One numeric series per period, switchable without another request.
    /// 每个周期一条数值序列，切换时不再发起请求。
    Chart {
        /// Plugin locale message key titling the chart.
        /// 图表标题使用的插件语言包消息键。
        #[serde(default)]
        title_message_key: Option<String>,
        /// Plugin locale message key naming the plotted value, used for the accessible label and point details.
        /// 命名所绘数值的插件语言包消息键，用于无障碍标签与数据点详情。
        value_label_message_key: String,
        /// Selectable periods; the first is selected initially.
        /// 可选周期；初始选中第一个。
        periods: Vec<VulcanPluginDataChartPeriod>,
        /// Numeric point path plotted as the bar or line height.
        /// 作为柱高或折线高度绘制的数值数据点路径。
        value_path: String,
        /// Host formatting of the plotted value.
        /// 所绘数值的宿主格式。
        #[serde(default = "default_number_format")]
        value_format: VulcanPluginDataFormat,
        /// Optional point value shown beside the plotted value in point details.
        /// 在数据点详情中显示于所绘数值旁的可选数据点值。
        #[serde(default)]
        detail: Option<VulcanPluginDataValue>,
        /// Number of most recent points shown.
        /// 显示的最近数据点数量。
        #[serde(default = "default_chart_limit")]
        limit: u32,
        /// Plugin locale message key shown when the selected period has no points.
        /// 所选周期没有数据点时显示的插件语言包消息键。
        empty_message_key: String,
        /// WebUI presentation hints.
        /// WebUI 呈现提示。
        #[serde(default)]
        web: VulcanPluginDataWebHints,
        /// Terminal UI presentation hints.
        /// 终端界面呈现提示。
        #[serde(default)]
        tui: VulcanPluginDataTuiHints,
    },
    /// Rows of one result array.
    /// 一个结果数组的各行。
    Table {
        /// Array path of the rows.
        /// 行所在的数组路径。
        path: String,
        /// Plugin locale message key titling the table.
        /// 表格标题使用的插件语言包消息键。
        #[serde(default)]
        title_message_key: Option<String>,
        /// Plugin locale message key with a `{{count}}` placeholder shown beside the title.
        /// 显示在标题旁、含 `{{count}}` 占位符的插件语言包消息键。
        #[serde(default)]
        count_message_key: Option<String>,
        /// Prepends a 1-based rank column.
        /// 在前方加入从 1 开始的名次列。
        #[serde(default)]
        rank: bool,
        /// Ordered columns, each relative to one row.
        /// 有序列，每列相对于单行。
        columns: Vec<VulcanPluginDataColumn>,
        /// Plugin locale message key shown when the array is empty.
        /// 数组为空时显示的插件语言包消息键。
        empty_message_key: String,
        /// WebUI presentation hints.
        /// WebUI 呈现提示。
        #[serde(default)]
        web: VulcanPluginDataWebHints,
        /// Terminal UI presentation hints.
        /// 终端界面呈现提示。
        #[serde(default)]
        tui: VulcanPluginDataTuiHints,
    },
}

/// One key figure: a path value or the ratio of two paths.
/// 一个关键数字：路径值或两个路径的比值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataMetric {
    /// Plugin locale message key for the figure label.
    /// 数字标签使用的插件语言包消息键。
    pub label_message_key: String,
    /// Value path; exclusive with `ratio`.
    /// 数值路径；与 `ratio` 互斥。
    #[serde(default)]
    pub path: Option<String>,
    /// Numerator and denominator paths; exclusive with `path` and shown with the `ratio` format.
    /// 分子与分母路径；与 `path` 互斥，并以 `ratio` 格式显示。
    #[serde(default)]
    pub ratio: Option<VulcanPluginDataRatio>,
    /// Host formatting of the value.
    /// 数值的宿主格式。
    #[serde(default = "default_number_format")]
    pub format: VulcanPluginDataFormat,
}

/// Two numeric paths whose quotient is shown.
/// 显示其商的两个数值路径。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataRatio {
    /// Numerator path.
    /// 分子路径。
    pub numerator: String,
    /// Denominator path; values below one count as one.
    /// 分母路径；小于一的值按一计算。
    pub denominator: String,
}

/// One selectable chart period.
/// 一个可选图表周期。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataChartPeriod {
    /// Stable chart-local period identifier.
    /// 稳定的图表内周期标识。
    pub id: String,
    /// Plugin locale message key for the period switch.
    /// 周期切换使用的插件语言包消息键。
    pub label_message_key: String,
    /// Array path of this period's points.
    /// 此周期数据点所在的数组路径。
    pub path: String,
    /// One or two point paths forming the axis label; two are shown as a range.
    /// 组成坐标轴标签的一个或两个数据点路径；两个时显示为区间。
    pub label_paths: Vec<String>,
    /// Host formatting of the axis label parts.
    /// 坐标轴标签各部分的宿主格式。
    #[serde(default)]
    pub label_format: VulcanPluginDataFormat,
}

/// One table column.
/// 一个表格列。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataColumn {
    /// Row-relative value path.
    /// 相对于行的数值路径。
    pub path: String,
    /// Plugin locale message key for the column header.
    /// 列标题使用的插件语言包消息键。
    pub label_message_key: String,
    /// Host formatting of the cell.
    /// 单元格的宿主格式。
    #[serde(default)]
    pub format: VulcanPluginDataFormat,
}

/// One formatted path value.
/// 一个带格式的路径值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataValue {
    /// Value path.
    /// 数值路径。
    pub path: String,
    /// Host formatting of the value.
    /// 数值的宿主格式。
    pub format: VulcanPluginDataFormat,
}

/// Host value formats; absent or null values render as an em dash on every surface.
/// 宿主数值格式；缺失或 null 值在所有界面上都显示为破折号。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginDataFormat {
    /// The value as text.
    /// 原样文本。
    #[default]
    Text,
    /// The value as inline code.
    /// 行内代码文本。
    Code,
    /// Locale-grouped number.
    /// 本地化分组数字。
    Number,
    /// Locale compact number such as 12K.
    /// 本地化紧凑数字，例如 12K。
    Compact,
    /// A 0–100 percentage with one decimal.
    /// 带一位小数的 0–100 百分比。
    Percent,
    /// A quotient shown as a multiplier; metrics with `ratio` only.
    /// 显示为倍数的商；仅限带 `ratio` 的指标。
    Ratio,
    /// Milliseconds as a compact duration.
    /// 毫秒显示为紧凑时长。
    DurationMs,
    /// An ISO `YYYY-MM-DD` calendar date, never shifted across time zones.
    /// ISO `YYYY-MM-DD` 日历日期，不跨时区偏移。
    Date,
    /// An ISO `YYYY-MM` calendar month.
    /// ISO `YYYY-MM` 日历月份。
    Month,
    /// Unix milliseconds as a local date and time.
    /// Unix 毫秒显示为本地日期时间。
    TimestampMs,
    /// Unix seconds as a local calendar date.
    /// Unix 秒显示为本地日历日期。
    DateUnixSec,
    /// A bar relative to the largest value of its column; table columns only.
    /// 相对于本列最大值的条形；仅限表格列。
    Bar,
}

/// Presentation hints read only by the WebUI.
/// 仅由 WebUI 读取的呈现提示。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataWebHints {
    /// Omits the block from the WebUI.
    /// 在 WebUI 中省略此数据块。
    #[serde(default)]
    pub hidden: bool,
    /// Metrics: `cards` (default) or `tiles`; chart: `bars` (default) or `line`.
    /// 指标：`cards`（默认）或 `tiles`；图表：`bars`（默认）或 `line`。
    #[serde(default)]
    pub variant: Option<VulcanPluginDataWebVariant>,
    /// Metrics only: fixed column count on wide pages; narrow pages always reflow.
    /// 仅限指标：宽页面上的固定列数；窄页面始终重新排布。
    #[serde(default)]
    pub columns: Option<u8>,
}

/// WebUI block variants.
/// WebUI 数据块变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginDataWebVariant {
    /// Metrics as separate cards with an icon slot.
    /// 指标显示为带图标位的独立卡片。
    Cards,
    /// Metrics as compact tinted tiles.
    /// 指标显示为紧凑着色方块。
    Tiles,
    /// Chart as vertical bars.
    /// 图表显示为竖直柱形。
    Bars,
    /// Chart as a line.
    /// 图表显示为折线。
    Line,
}

/// Presentation hints read only by a terminal UI.
/// 仅由终端界面读取的呈现提示。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub struct VulcanPluginDataTuiHints {
    /// Omits the block from a terminal UI.
    /// 在终端界面中省略此数据块。
    #[serde(default)]
    pub hidden: bool,
    /// Chart only: `sparkline` (default) or `table`.
    /// 仅限图表：`sparkline`（默认）或 `table`。
    #[serde(default)]
    pub variant: Option<VulcanPluginDataTuiVariant>,
}

/// Terminal UI chart variants.
/// 终端界面图表变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "manifest-schema", derive(schemars::JsonSchema))]
pub enum VulcanPluginDataTuiVariant {
    /// One-line block-character trend.
    /// 单行块字符趋势。
    Sparkline,
    /// Period label and value rows.
    /// 周期标签与数值行。
    Table,
}

fn default_number_format() -> VulcanPluginDataFormat {
    VulcanPluginDataFormat::Number
}

fn default_chart_limit() -> u32 {
    14
}

/// Validate one data section's source and blocks against the read action's input and output schemas.
/// 依据读取 Action 的输入与输出 Schema 校验一个数据区块的来源与数据块。
/// message checks one fallback-locale message key; returns the first concrete violation.
/// message 检查一个回退语言消息键；返回首个具体违规。
pub(super) fn validate_data_section(
    mount_id: &str,
    section_id: &str,
    source: &VulcanPluginDataSource,
    blocks: &[VulcanPluginDataBlock],
    context: &SettingsLayoutContext<'_>,
    message: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let fail = |text: String| {
        Err(format!(
            "Plugin settings mount `{mount_id}`: data section `{section_id}` {text}"
        ))
    };
    let Some(contract) = context
        .actions
        .get(source.action.as_str())
        .filter(|contract| {
            contract.manifest.effect == VulcanPluginActionEffect::Read
                && contract
                    .manifest
                    .callers
                    .contains(&VulcanPluginActionCaller::PluginUi)
        })
    else {
        return fail(format!(
            "action `{}` must be declared with effect `read` and the `plugin_ui` caller.",
            source.action
        ));
    };
    validate_arguments(
        mount_id,
        section_id,
        source,
        contract.input.as_ref(),
        message,
    )?;
    let output = contract.output.as_ref();
    let paths = PathChecker {
        mount_id,
        section_id,
        root: output,
    };
    match (&source.available_path, &source.unavailable_message_key) {
        (Some(path), Some(key)) => {
            paths.check(output, path, ValueKind::Boolean)?;
            message(key)?;
        }
        (None, None) => {}
        _ => {
            return fail(
                "must declare `available_path` and `unavailable_message_key` together.".into(),
            );
        }
    }
    if let Some(path) = &source.error_path {
        if source.available_path.is_none() {
            return fail("may declare `error_path` only with `available_path`.".into());
        }
        paths.check(output, path, ValueKind::Text)?;
    }
    let mut topics = BTreeSet::new();
    for topic in &source.refresh_topics {
        if topic.is_empty()
            || topic.len() > 128
            || !topic
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            || !topics.insert(topic)
        {
            return fail(format!(
                "refresh topic `{topic}` must be a distinct dotted identifier."
            ));
        }
    }
    if blocks.is_empty() {
        return fail("must declare at least one block.".into());
    }
    for block in blocks {
        validate_block(&paths, output, block, message)?;
        if let VulcanPluginDataBlock::Table {
            count_message_key: Some(key),
            ..
        } = block
            && context
                .messages
                .get(key)
                .is_some_and(|text| !text.contains(VULCAN_PLUGIN_DATA_COUNT_PLACEHOLDER))
        {
            return fail(format!(
                "table count message `{key}` must contain `{VULCAN_PLUGIN_DATA_COUNT_PLACEHOLDER}`."
            ));
        }
    }
    Ok(())
}

/// Validate the argument controls and that every option combination forms a valid input object.
/// 校验参数控件，并确认每个选项组合都构成有效输入对象。
fn validate_arguments(
    mount_id: &str,
    section_id: &str,
    source: &VulcanPluginDataSource,
    input: Option<&Value>,
    message: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let fail = |text: String| {
        Err(format!(
            "Plugin settings mount `{mount_id}`: data section `{section_id}` {text}"
        ))
    };
    let defaults = source
        .arguments
        .iter()
        .map(|argument| (argument.name.clone(), argument.default.clone()))
        .collect::<Map<_, _>>();
    if defaults.len() != source.arguments.len() {
        return fail("repeats an argument name.".into());
    }
    let label = format!("{section_id} arguments");
    match input {
        None if source.arguments.is_empty() => return Ok(()),
        None => return fail("declares arguments for an action without an input schema.".into()),
        Some(schema) => validate_value(mount_id, schema, &Value::Object(defaults.clone()), &label)?,
    }
    for argument in &source.arguments {
        message(&argument.label_message_key)?;
        if argument.options.len() < 2 {
            return fail(format!(
                "argument `{}` must offer at least two options.",
                argument.name
            ));
        }
        let mut seen = BTreeSet::new();
        for option in &argument.options {
            if !(option.value.is_string() || option.value.is_i64() || option.value.is_u64())
                || !seen.insert(option.value.to_string())
            {
                return fail(format!(
                    "argument `{}` options must be distinct string or integer values.",
                    argument.name
                ));
            }
            if let Some(key) = &option.label_message_key {
                message(key)?;
            }
            let mut candidate = defaults.clone();
            candidate.insert(argument.name.clone(), option.value.clone());
            validate_value(
                mount_id,
                input.expect("checked above"),
                &Value::Object(candidate),
                &label,
            )?;
        }
        if !argument
            .options
            .iter()
            .any(|option| option.value == argument.default)
        {
            return fail(format!(
                "argument `{}` default must be one of its options.",
                argument.name
            ));
        }
    }
    Ok(())
}

/// Validate one block's messages, paths, formats and surface hints.
/// 校验单个数据块的文案、路径、格式与界面提示。
fn validate_block(
    paths: &PathChecker<'_>,
    output: Option<&Value>,
    block: &VulcanPluginDataBlock,
    message: &dyn Fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let optional = |key: &Option<String>| key.as_deref().map_or(Ok(()), message);
    match block {
        VulcanPluginDataBlock::Metrics {
            title_message_key,
            help_message_key,
            source_path,
            empty_message_key,
            items,
            web,
            tui,
        } => {
            optional(title_message_key)?;
            optional(help_message_key)?;
            if help_message_key.is_some() && title_message_key.is_none() {
                return paths.fail("metrics help requires a title.");
            }
            if source_path.is_some() != empty_message_key.is_some() {
                return paths
                    .fail("metrics must declare `source_path` and `empty_message_key` together.");
            }
            optional(empty_message_key)?;
            let base = match source_path {
                Some(path) => paths.check(output, path, ValueKind::Object)?,
                None => output,
            };
            if items.is_empty() {
                return paths.fail("metrics must declare at least one item.");
            }
            for item in items {
                message(&item.label_message_key)?;
                match (&item.path, &item.ratio) {
                    (Some(path), None) => {
                        if matches!(
                            item.format,
                            VulcanPluginDataFormat::Ratio | VulcanPluginDataFormat::Bar
                        ) {
                            return paths.fail(
                                "metric formats `ratio` and `bar` are not valid for a path value.",
                            );
                        }
                        paths.check(base, path, ValueKind::of(item.format))?;
                    }
                    (None, Some(ratio)) => {
                        if item.format != VulcanPluginDataFormat::Ratio {
                            return paths.fail("a ratio metric must use the `ratio` format.");
                        }
                        paths.check(base, &ratio.numerator, ValueKind::Number)?;
                        paths.check(base, &ratio.denominator, ValueKind::Number)?;
                    }
                    _ => {
                        return paths
                            .fail("each metric must declare exactly one of `path` and `ratio`.");
                    }
                }
            }
            validate_hints(paths, web, tui, HintTarget::Metrics)
        }
        VulcanPluginDataBlock::Chart {
            title_message_key,
            value_label_message_key,
            periods,
            value_path,
            value_format,
            detail,
            limit,
            empty_message_key,
            web,
            tui,
        } => {
            optional(title_message_key)?;
            message(value_label_message_key)?;
            message(empty_message_key)?;
            if !(1..=VULCAN_PLUGIN_DATA_CHART_MAX_POINTS).contains(limit) {
                return paths.fail(&format!(
                    "chart limit must be between 1 and {VULCAN_PLUGIN_DATA_CHART_MAX_POINTS}."
                ));
            }
            if !matches!(
                value_format,
                VulcanPluginDataFormat::Number
                    | VulcanPluginDataFormat::Compact
                    | VulcanPluginDataFormat::Percent
                    | VulcanPluginDataFormat::DurationMs
            ) {
                return paths.fail("chart values must use a numeric format.");
            }
            if periods.is_empty() {
                return paths.fail("chart must declare at least one period.");
            }
            let mut ids = BTreeSet::new();
            for period in periods {
                crate::catalog::validate_identifier(&period.id, "data chart period id")?;
                if !ids.insert(period.id.as_str()) {
                    return paths.fail(&format!("repeats chart period `{}`.", period.id));
                }
                message(&period.label_message_key)?;
                if !(1..=2).contains(&period.label_paths.len()) {
                    return paths.fail("chart periods need one or two label paths.");
                }
                if !matches!(
                    period.label_format,
                    VulcanPluginDataFormat::Text
                        | VulcanPluginDataFormat::Date
                        | VulcanPluginDataFormat::Month
                ) {
                    return paths.fail(
                        "chart period labels must use the `text`, `date` or `month` format.",
                    );
                }
                let point = paths.check_rows(output, &period.path)?;
                for path in &period.label_paths {
                    paths.check(point, path, ValueKind::Text)?;
                }
                paths.check(point, value_path, ValueKind::Number)?;
                if let Some(detail) = detail {
                    if matches!(
                        detail.format,
                        VulcanPluginDataFormat::Ratio | VulcanPluginDataFormat::Bar
                    ) {
                        return paths
                            .fail("chart details may not use the `ratio` or `bar` format.");
                    }
                    paths.check(point, &detail.path, ValueKind::of(detail.format))?;
                }
            }
            validate_hints(paths, web, tui, HintTarget::Chart)
        }
        VulcanPluginDataBlock::Table {
            path,
            title_message_key,
            count_message_key,
            columns,
            empty_message_key,
            web,
            tui,
            ..
        } => {
            optional(title_message_key)?;
            message(empty_message_key)?;
            if let Some(key) = count_message_key {
                message(key)?;
                if title_message_key.is_none() {
                    return paths.fail("a table count requires a title.");
                }
            }
            let row = paths.check_rows(output, path)?;
            if columns.is_empty() {
                return paths.fail("table must declare at least one column.");
            }
            for column in columns {
                message(&column.label_message_key)?;
                if column.format == VulcanPluginDataFormat::Ratio {
                    return paths.fail("table columns may not use the `ratio` format.");
                }
                paths.check(row, &column.path, ValueKind::of(column.format))?;
            }
            validate_hints(paths, web, tui, HintTarget::Table)
        }
    }
}

/// Block kinds that accept different hints.
/// 接受不同提示的数据块类型。
#[derive(Clone, Copy, PartialEq, Eq)]
enum HintTarget {
    Metrics,
    Chart,
    Table,
}

/// Validate that each surface hint applies to the block kind it is declared on.
/// 校验每个界面提示都适用于其所在数据块的类型。
fn validate_hints(
    paths: &PathChecker<'_>,
    web: &VulcanPluginDataWebHints,
    tui: &VulcanPluginDataTuiHints,
    target: HintTarget,
) -> Result<(), String> {
    let web_variant_fits = match web.variant {
        None => true,
        Some(VulcanPluginDataWebVariant::Cards | VulcanPluginDataWebVariant::Tiles) => {
            target == HintTarget::Metrics
        }
        Some(VulcanPluginDataWebVariant::Bars | VulcanPluginDataWebVariant::Line) => {
            target == HintTarget::Chart
        }
    };
    if !web_variant_fits {
        return paths.fail("uses a WebUI variant that does not apply to its block type.");
    }
    if let Some(columns) = web.columns
        && (target != HintTarget::Metrics
            || !(1..=VULCAN_PLUGIN_DATA_MAX_METRIC_COLUMNS).contains(&columns))
    {
        return paths.fail(&format!("WebUI columns apply only to metrics and must be between 1 and {VULCAN_PLUGIN_DATA_MAX_METRIC_COLUMNS}."));
    }
    if tui.variant.is_some() && target != HintTarget::Chart {
        return paths.fail("uses a terminal UI variant that applies only to charts.");
    }
    Ok(())
}

/// Value kinds a path is expected to hold.
/// 路径预期持有的值类型。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Any,
    Text,
    Number,
    Boolean,
    Object,
    Array,
}

impl ValueKind {
    /// Kind a format reads.
    /// 某格式读取的值类型。
    fn of(format: VulcanPluginDataFormat) -> Self {
        match format {
            VulcanPluginDataFormat::Text | VulcanPluginDataFormat::Code => Self::Any,
            VulcanPluginDataFormat::Date | VulcanPluginDataFormat::Month => Self::Text,
            _ => Self::Number,
        }
    }

    /// Whether a declared JSON type name, or every non-null name of a union, fits this kind.
    /// 声明的 JSON 类型名（或联合类型中每个非 null 名称）是否符合此类型。
    fn accepts(self, declared: &Value) -> bool {
        let fits = |name: &str| match self {
            Self::Any => name != "object" && name != "array",
            Self::Text => name == "string",
            Self::Number => name == "integer" || name == "number",
            Self::Boolean => name == "boolean",
            Self::Object => name == "object",
            Self::Array => name == "array",
        };
        match declared {
            Value::String(name) => fits(name),
            Value::Array(names) => names
                .iter()
                .filter_map(Value::as_str)
                .filter(|name| *name != "null")
                .all(fits),
            _ => true,
        }
    }
}

/// Resolves dotted result paths against the action's output schema where it is specific enough.
/// 在输出 Schema 足够具体时，依据它解析点分结果路径。
struct PathChecker<'a> {
    mount_id: &'a str,
    section_id: &'a str,
    root: Option<&'a Value>,
}

impl<'a> PathChecker<'a> {
    fn fail<T>(&self, text: &str) -> Result<T, String> {
        Err(format!(
            "Plugin settings mount `{}`: data section `{}` {text}",
            self.mount_id, self.section_id
        ))
    }

    /// Follow a local `$ref` once; other nodes are returned unchanged.
    /// 跟随一次本地 `$ref`；其他节点原样返回。
    fn deref(&self, node: &'a Value) -> &'a Value {
        let target = node
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| {
                let name = reference
                    .strip_prefix("#/$defs/")
                    .or_else(|| reference.strip_prefix("#/definitions/"))?;
                let defs = if reference.starts_with("#/$defs/") {
                    "$defs"
                } else {
                    "definitions"
                };
                self.root?.get(defs)?.get(name)
            });
        target.unwrap_or(node)
    }

    /// Resolve path below schema and check its declared type; returns the target schema when known.
    /// 在 schema 之下解析 path 并检查其声明类型；已知时返回目标 Schema。
    fn check(
        &self,
        schema: Option<&'a Value>,
        path: &str,
        kind: ValueKind,
    ) -> Result<Option<&'a Value>, String> {
        let valid_segment = |segment: &str| {
            segment
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        };
        if !path.split('.').all(valid_segment) {
            return self.fail(&format!("path `{path}` must be dot-separated identifiers."));
        }
        let mut node = schema.map(|schema| self.deref(schema));
        for segment in path.split('.') {
            let Some(current) = node else { break };
            node = match current.get("properties").and_then(Value::as_object) {
                Some(properties) => match properties.get(segment) {
                    Some(next) => Some(self.deref(next)),
                    None if current.get("additionalProperties") == Some(&Value::Bool(false)) => {
                        return self.fail(&format!(
                            "path `{path}` is not in the action output schema."
                        ));
                    }
                    None => None,
                },
                None => None,
            };
        }
        if let Some(declared) = node.and_then(|target| target.get("type"))
            && !kind.accepts(declared)
        {
            return self.fail(&format!(
                "path `{path}` has an output schema type that its format cannot show."
            ));
        }
        Ok(node)
    }

    /// Resolve an array path and return its row schema when known.
    /// 解析数组路径，并在已知时返回其行 Schema。
    fn check_rows(
        &self,
        schema: Option<&'a Value>,
        path: &str,
    ) -> Result<Option<&'a Value>, String> {
        let array = self.check(schema, path, ValueKind::Array)?;
        Ok(array
            .and_then(|array| array.get("items"))
            .map(|items| self.deref(items)))
    }
}
