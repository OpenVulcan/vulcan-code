//! AI capability selection and editor state extracted from the main workbench module.
//! 从主工作台模块中抽取的 AI 能力选择与编辑状态。

use crate::provider_config::{AiCapabilityVendorConnectionMode, AiToolKind};
use std::collections::BTreeMap;

/// Capability sections rendered inside the workbench quick capability panel opened from the composer controls.
/// 渲染在提示词控制区快捷能力面板中的能力分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkbenchAiCapabilitySection {
    /// Unified hosted or third-party web-search capability routing.
    /// 统一原生或第三方网页搜索能力路由。
    WebSearch,
    /// Image-understanding capability routing delegated to external vision vendors.
    /// 委派给外部视觉供应商的图片理解能力路由。
    UnderstandImage,
    /// Video-understanding placeholder section.
    /// 视频理解占位分区。
    UnderstandVideo,
    /// Audio-understanding placeholder section.
    /// 音频理解占位分区。
    UnderstandAudio,
    /// Image-generation capability routing.
    /// 图片生成能力路由。
    ImageGeneration,
    /// Video-generation capability routing.
    /// 视频生成能力路由。
    VideoGeneration,
    /// Speech-synthesis capability routing.
    /// 语音合成能力路由。
    SpeechSynthesis,
    /// Music-generation capability routing.
    /// 音乐生成能力路由。
    MusicGeneration,
    /// Music-cover capability routing.
    /// 音乐翻唱能力路由。
    MusicCover,
}

impl WorkbenchAiCapabilitySection {
    /// Returns all quick-panel sections in stable order.
    /// 返回快捷能力面板按稳定顺序展示的全部分区。
    pub(super) const ALL: [Self; 9] = [
        Self::WebSearch,
        Self::UnderstandImage,
        Self::UnderstandVideo,
        Self::UnderstandAudio,
        Self::ImageGeneration,
        Self::VideoGeneration,
        Self::SpeechSynthesis,
        Self::MusicGeneration,
        Self::MusicCover,
    ];

    /// Returns the compact function label rendered in the quick capability panel.
    /// 返回渲染在快捷能力面板中的紧凑功能标签。
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::WebSearch => "联网搜索",
            Self::UnderstandImage => "图像理解",
            Self::UnderstandVideo => "视频理解",
            Self::UnderstandAudio => "音频理解",
            Self::ImageGeneration => "图像生成",
            Self::VideoGeneration => "视频生成",
            Self::SpeechSynthesis => "语音合成",
            Self::MusicGeneration => "音乐生成",
            Self::MusicCover => "音乐翻唱",
        }
    }

    /// Returns the semantic capability kind already implemented by the current section when one exists.
    /// 返回当前分区已经具备实现时对应的语义能力类型。
    pub(super) const fn capability_kind(self) -> Option<AiToolKind> {
        match self {
            Self::WebSearch => Some(AiToolKind::WebSearch),
            Self::UnderstandImage => Some(AiToolKind::UnderstandImage),
            Self::UnderstandVideo => None,
            Self::UnderstandAudio => None,
            Self::ImageGeneration => Some(AiToolKind::ImageGeneration),
            Self::VideoGeneration => Some(AiToolKind::VideoGeneration),
            Self::SpeechSynthesis => Some(AiToolKind::SpeechSynthesis),
            Self::MusicGeneration => Some(AiToolKind::MusicGeneration),
            Self::MusicCover => Some(AiToolKind::MusicCover),
        }
    }

    /// Returns whether the current section should show one routing-strategy block above the target list.
    /// 返回当前分区是否应在目标列表上方展示路由策略区块。
    pub(super) const fn supports_routing_strategy(self) -> bool {
        matches!(
            self,
            Self::WebSearch | Self::UnderstandImage | Self::UnderstandVideo | Self::UnderstandAudio
        )
    }
}

/// Temporary in-memory picker state used by one capability page when the user selects targets from configured vendors.
/// 某个能力页在用户从已配置供应商中选择目标时使用的临时内存选择器状态。
#[derive(Debug, Clone)]
pub(super) struct AiCapabilityTargetPickerState {
    /// Semantic capability currently being configured by the picker.
    /// 当前由选择器配置的语义能力。
    pub(super) capability_kind: AiToolKind,
    /// Vendor currently highlighted in the left-side picker list.
    /// 当前在选择器左侧列表中高亮的供应商。
    pub(super) selected_vendor_id: Option<String>,
    /// Per-vendor selected target map keyed by `target_alias` with its requested model override.
    /// 以 `target_alias` 为键、以请求模型覆盖值为值的按供应商分组已选目标映射。
    pub(super) selections: BTreeMap<String, BTreeMap<String, String>>,
}

impl AiCapabilityTargetPickerState {
    /// Returns the total number of checked targets currently staged inside the picker.
    /// 返回当前在选择器中暂存勾选的目标总数。
    pub(super) fn total_selected_count(&self) -> usize {
        self.selections.values().map(BTreeMap::len).sum()
    }
}

/// Temporary in-memory picker state used by one capability page when the user chooses a routing strategy from a compact selector.
/// 某个能力页在用户从紧凑选择器中挑选路由策略时使用的临时内存弹窗状态。
#[derive(Debug, Clone)]
pub(super) struct AiCapabilityRoutingPickerState {
    /// Semantic capability currently owning the routing strategy being edited.
    /// 当前持有待编辑路由策略的语义能力。
    pub(super) capability_kind: AiToolKind,
}

/// Blocking vendor picker state used by the supplier page before one new shared AI capability vendor instance is created.
/// 供应商页在创建新的共享 AI 能力供应商实例前使用的阻塞式供应商选择器状态。
#[derive(Debug, Clone, Default)]
pub(super) struct AiCapabilityVendorPickerState {
    /// Live keyword filter applied to the vendor catalog list.
    /// 应用于供应商目录列表的实时关键字过滤值。
    pub(super) filter: String,
}

/// Blocking vendor editor state used when one shared AI capability vendor instance is added or updated.
/// 新增或更新共享 AI 能力供应商实例时使用的阻塞式供应商编辑器状态。
#[derive(Debug, Clone)]
pub(super) struct AiCapabilityVendorEditorState {
    /// Stable shared vendor identifier currently being edited.
    /// 当前正在编辑的稳定共享供应商标识。
    pub(super) vendor_id: String,
    /// Shared vendor display name shown in the modal header.
    /// 显示在弹窗标题中的共享供应商名称。
    pub(super) display_name: String,
    /// Shared API key or token staged in the editor.
    /// 在编辑器中暂存的共享 API Key 或 Token。
    pub(super) api_key: String,
    /// Optional local/cloud connection mode staged in the editor for vendors that support explicit host switching.
    /// 为支持显式宿主切换的供应商在编辑器中暂存的本地/云端连接模式。
    pub(super) connection_mode: AiCapabilityVendorConnectionMode,
    /// Shared vendor-level base URL override staged in the editor.
    /// 在编辑器中暂存的共享供应商级 Base URL 覆盖值。
    pub(super) base_url: String,
    /// Shared region or locale staged in the editor.
    /// 在编辑器中暂存的共享区域或地区值。
    pub(super) region: String,
    /// Optional fixed region choices rendered as a dropdown when the provider only supports a known finite set.
    /// 当提供方只支持已知有限集合时渲染为下拉框的可选固定区域列表。
    pub(super) region_options: Vec<String>,
    /// Whether the current editor session is creating a brand-new vendor entry instead of updating an existing one.
    /// 当前编辑会话是否正在创建全新供应商条目，而不是更新已有条目。
    pub(super) is_new: bool,
}
