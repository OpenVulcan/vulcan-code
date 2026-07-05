//! Rendering registry, panel models, and provider-facing block assembly.
//! 渲染注册表、面板模型与面向 Provider 的块组装。

use crate::types::{InjectionKind, InjectionStateGroup, InjectionStateItem};
use crate::util::normalize_renderer_id;
use serde_json::Value;
use std::collections::BTreeMap;

/// Provider-facing resolved injection view.
/// 面向 Provider 的解析后注入视图。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedInjectionView {
    /// Persistent hidden user blocks rendered as clean tag text.
    /// 渲染为干净标签文本的永久隐藏用户块。
    pub persistent_blocks: Vec<String>,
    /// Transient blocks that should prefix the target user prompt.
    /// 应前置到目标用户提示词的临时块。
    pub transient_prefix: String,
    /// Whether transient content is present in the current view.
    /// 当前视图是否存在临时内容。
    pub has_transient: bool,
    /// Stable fingerprint for diagnostics and continuation safety checks.
    /// 用于诊断与 continuation 安全检查的稳定指纹。
    pub fingerprint: String,
}

/// Reads a VMM relevance score from numeric or string metadata.
/// 从数字或字符串元数据中读取 VMM 相关性评分。
pub(crate) fn vmm_metadata_score_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

/// One simplified side-panel card for a rendered tag.
/// 一个渲染标签对应的简洁侧面板卡片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionPanelCard {
    /// Tag name shown as the card title.
    /// 作为卡片标题展示的标签名。
    pub tag: String,
    /// Tag description shown below the title.
    /// 展示在标题下方的标签说明。
    pub description: String,
    /// Effective items shown inside the card.
    /// 展示在卡片内部的有效内容项。
    pub items: Vec<InjectionPanelItem>,
}

/// One simplified side-panel item for active injection content.
/// 一个活动注入内容对应的简洁侧面板条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionPanelItem {
    /// Compact item content shown directly inside the panel.
    /// 直接显示在面板中的紧凑内容项。
    pub content: String,
    /// Detailed hover text assembled by the renderer.
    /// 由渲染器组装的悬停详情文本。
    pub detail: String,
    /// Stable item dedupe key used for precise current-session suppression.
    /// 用于精确执行当前会话抑制的稳定条目去重键。
    pub dedupe_key: Option<String>,
    /// Durable VMM memory id when the item originates from a memory row.
    /// 当条目来源于长期记忆行时对应的 VMM 记忆标识。
    pub memory_id: Option<u64>,
    /// Durable source turn id when the item can be traced back to a dialogue turn.
    /// 当条目可回查到对话回合时对应的长期来源回合标识。
    pub turn_id: Option<u64>,
    /// Human-readable relevance score label prepared for UI display.
    /// 为界面展示准备的人类可读相关性评分标签。
    pub score_label: Option<String>,
    /// Whether the backing VMM row can be queried for full turn detail.
    /// 背后的 VMM 行是否可查询完整回合详情。
    pub has_dialogue: bool,
    /// Optional original creation datetime from the VMM record.
    /// 来自 VMM 记录的可选原始创建时间。
    pub created_datetime: Option<String>,
    /// Optional injection item update timestamp.
    /// 可选注入条目更新时间戳。
    pub updated_at: Option<String>,
    /// Optional turn-origin label shown in the panel.
    /// 面板中展示的可选回合来源标签。
    pub turn_label: Option<String>,
    /// Human-readable TTL label shown in muted text.
    /// 以灰色文字展示的人类可读 TTL 标签。
    pub ttl_label: String,
    /// Optional source label shown in diagnostics and UI only.
    /// 仅在诊断和界面中展示的可选来源标签。
    pub source_display: Option<String>,
}

/// Options used when rendering provider-visible injection blocks.
/// 渲染 Provider 可见注入块时使用的选项。
pub struct InjectionRenderOptions<'a> {
    /// Renderer registry used for structured plugin metadata.
    /// 用于结构化插件元数据的渲染器注册表。
    pub registry: &'a InjectionRendererRegistry,
    /// Current logical turn marker used to split transient items.
    /// 用于拆分临时内容项的当前逻辑回合标记。
    pub current_turn_marker: Option<&'a str>,
}

impl Default for InjectionRenderOptions<'_> {
    /// Builds default render options with no custom renderer and no current turn marker.
    /// 构建无自定义渲染器且无当前回合标记的默认渲染选项。
    fn default() -> Self {
        Self {
            registry: &DEFAULT_RENDERER_REGISTRY,
            current_turn_marker: None,
        }
    }
}

/// Empty renderer registry used by default render options.
/// 默认渲染选项使用的空渲染器注册表。
static DEFAULT_RENDERER_REGISTRY: InjectionRendererRegistry = InjectionRendererRegistry {
    renderers: BTreeMap::new(),
};

/// Renderer callback set for one structured injection item type.
/// 单类结构化注入内容项的渲染回调集合。
#[derive(Clone, Copy)]
pub struct InjectionItemRenderer {
    /// Renders one item for provider-visible model context.
    /// 为模型可见上下文渲染单条内容项。
    pub render_model: fn(&InjectionStateItem) -> String,
    /// Renders one compact panel row.
    /// 渲染一条紧凑面板行。
    pub render_panel: fn(&InjectionStateItem) -> String,
    /// Renders one detailed panel tooltip body.
    /// 渲染一段详细面板悬停正文。
    pub render_detail: fn(&InjectionStateItem) -> String,
}

/// Registry that maps structured injection renderer ids to renderers.
/// 将结构化注入渲染器标识映射到渲染器的注册表。
#[derive(Default)]
pub struct InjectionRendererRegistry {
    /// Registered renderers keyed by renderer id.
    /// 按渲染器标识索引的已注册渲染器。
    renderers: BTreeMap<String, InjectionItemRenderer>,
}

impl InjectionRendererRegistry {
    /// Registers one renderer id and returns the registry for chaining.
    /// 注册一个渲染器标识，并返回注册表以便链式调用。
    pub fn with_renderer(mut self, id: impl Into<String>, renderer: InjectionItemRenderer) -> Self {
        self.register(id, renderer);
        self
    }

    /// Registers or replaces one renderer id.
    /// 注册或替换一个渲染器标识。
    pub fn register(&mut self, id: impl Into<String>, renderer: InjectionItemRenderer) {
        let id = normalize_renderer_id(&id.into());
        if !id.is_empty() {
            self.renderers.insert(id, renderer);
        }
    }

    /// Renders one item for provider-visible model context.
    /// 为模型可见上下文渲染一条内容项。
    pub fn model_content(&self, item: &InjectionStateItem) -> String {
        self.renderer_for(item)
            .map(|renderer| (renderer.render_model)(item))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| item.content.clone())
    }

    /// Renders one compact panel row.
    /// 渲染一条紧凑面板行。
    pub fn panel_content(&self, item: &InjectionStateItem) -> String {
        self.renderer_for(item)
            .map(|renderer| (renderer.render_panel)(item))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| item.content.clone())
    }

    /// Renders one detailed panel tooltip body.
    /// 渲染一段详细面板悬停正文。
    pub fn panel_detail(&self, item: &InjectionStateItem) -> String {
        self.renderer_for(item)
            .map(|renderer| (renderer.render_detail)(item))
            .filter(|content| !content.trim().is_empty())
            .unwrap_or_else(|| item.content.clone())
    }

    /// Returns the renderer registered for one item.
    /// 返回一条内容项注册的渲染器。
    fn renderer_for(&self, item: &InjectionStateItem) -> Option<&InjectionItemRenderer> {
        item.renderer_id
            .as_deref()
            .and_then(|id| self.renderers.get(id))
    }
}

/// Renders one group into provider-facing tag text.
/// 将一个分组渲染为面向 Provider 的标签文本。
pub(crate) fn render_group_block(
    group: &InjectionStateGroup,
    options: &InjectionRenderOptions<'_>,
) -> Option<String> {
    if group.items.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push(format!("<{}>", group.tag));
    let description = group.description.trim();
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    match group.kind {
        InjectionKind::Persistent => {
            for item in group.items.values() {
                let content = options.registry.model_content(item);
                if !content.trim().is_empty() {
                    lines.push(content);
                }
            }
        }
        InjectionKind::Transient => {
            append_transient_section(
                &mut lines,
                "CURRENT_TURN",
                group.items.values().filter(|item| {
                    item_turn_origin(item, options.current_turn_marker)
                        == InjectionTurnOrigin::Current
                }),
                options.registry,
            );
            append_transient_section(
                &mut lines,
                "RETAINED_FROM_EARLIER_TURNS",
                group.items.values().filter(|item| {
                    item_turn_origin(item, options.current_turn_marker)
                        == InjectionTurnOrigin::Retained
                }),
                options.registry,
            );
        }
    }
    lines.push(format!("</{}>", group.tag));
    Some(lines.join("\n"))
}

/// Appends one transient turn-origin section when it contains renderable items.
/// 当临时回合来源分段包含可渲染内容项时追加该分段。
fn append_transient_section<'a>(
    lines: &mut Vec<String>,
    section_tag: &str,
    items: impl Iterator<Item = &'a InjectionStateItem>,
    registry: &InjectionRendererRegistry,
) {
    let contents = items
        .map(|item| registry.model_content(item))
        .filter(|content| !content.trim().is_empty())
        .collect::<Vec<_>>();
    if contents.is_empty() {
        return;
    }
    lines.push(format!("<{section_tag}>"));
    lines.extend(contents);
    lines.push(format!("</{section_tag}>"));
}

/// Provider-visible transient turn-origin category.
/// Provider 可见临时注入的回合来源分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InjectionTurnOrigin {
    /// Item belongs to the current user turn.
    /// 内容项属于当前用户回合。
    Current,
    /// Item was retained from an earlier user turn.
    /// 内容项从更早用户回合保留下来。
    Retained,
}

/// Classifies one item by comparing its marker to the current turn marker.
/// 通过比较内容项标记与当前回合标记对其分类。
fn item_turn_origin(
    item: &InjectionStateItem,
    current_turn_marker: Option<&str>,
) -> InjectionTurnOrigin {
    let Some(current_turn_marker) = current_turn_marker
        .map(str::trim)
        .filter(|marker| !marker.is_empty())
    else {
        return if item.last_consumed_turn_id.is_some() {
            InjectionTurnOrigin::Retained
        } else {
            InjectionTurnOrigin::Current
        };
    };
    if item.created_turn_marker.as_deref() == Some(current_turn_marker) {
        InjectionTurnOrigin::Current
    } else {
        InjectionTurnOrigin::Retained
    }
}

/// Builds a short human label for one item's turn origin.
/// 为内容项回合来源构建简短人类标签。
pub(crate) fn item_turn_label(
    item: &InjectionStateItem,
    current_turn_marker: Option<&str>,
) -> Option<String> {
    item.created_turn_marker.as_ref()?;
    Some(match item_turn_origin(item, current_turn_marker) {
        InjectionTurnOrigin::Current => String::from("当前轮次"),
        InjectionTurnOrigin::Retained => String::from("历史保留"),
    })
}
