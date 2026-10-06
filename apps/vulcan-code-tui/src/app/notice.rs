// Notice components: transient toasts, notices derived from live state and, later, notices that
// plugins declare all share one declarative description and one renderer (`ui::notice`).
// 通知组件：短暂提示、由实时状态派生的通知，以及日后插件声明的通知，共用同一个声明式描述与同一个
// 渲染器（`ui::notice`）。
//
// A description names theme tones, never colors, so every notice follows the active theme. The
// JSON shape below is the component declaration format; `Component::parse` reads it and refuses
// unknown component types and fields. Built-in notices are declared the same way in
// `notice-presets.json` and only supply their text in code.
// 描述只使用主题色调名称而不是颜色，使每个通知都跟随当前主题。下方 JSON 形状即组件声明格式；
// `Component::parse` 读取它，并拒绝未知的组件类型与字段。内置通知同样在 `notice-presets.json` 中
// 声明，代码只提供其文本。

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use super::App;
use super::pages::Tone;
use crate::model::{RunPhase, Severity};
use crate::tr;

/// How long a pushed notice stays when it names no duration.
/// 推送的通知未指定时长时的停留时间。
pub const NOTICE_MS: u64 = 3200;

/// Pushed notices kept at once; the oldest leaves first.
/// 同时保留的推送通知数；最早的先离开。
pub const NOTICE_LIMIT: usize = 3;

/// Corner or edge of the body a notice is anchored to; notices at one anchor stack away from it.
/// 通知锚定的主体角落或边缘；同一锚点的通知自该处依次排开。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    #[default]
    TopRight,
    TopLeft,
    TopCenter,
    BottomRight,
    BottomLeft,
    BottomCenter,
}

impl Placement {
    /// Whether the anchor is the bottom edge, so stacking grows upward.
    /// 锚点是否位于底边，此时向上堆叠。
    pub fn bottom(self) -> bool {
        matches!(
            self,
            Placement::BottomRight | Placement::BottomLeft | Placement::BottomCenter
        )
    }
}

/// Outline of a notice.
/// 通知的外框。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// A colored bar on the leading edge.
    /// 前缘的着色竖条。
    #[default]
    Bar,
    /// A full border whose top edge carries the title.
    /// 完整边框，标题位于上边。
    Box,
    /// No outline, only the popover background.
    /// 无外框，只有浮层背景。
    Plain,
}

/// One notice as declared: text, tones, anchor, outline and lifetime.
/// 一个已声明的通知：文本、色调、锚点、外框与存续时间。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeSpec {
    /// Identity that replaces an earlier notice with the same key.
    /// 用于替换同键旧通知的身份。
    #[serde(default)]
    pub key: Option<String>,
    /// Optional title above, or in the border of, the message.
    /// 消息上方或边框中的可选标题。
    #[serde(default)]
    pub title: Option<String>,
    /// Message text; presets leave it to the code that shows them.
    /// 消息文本；预设留给显示它们的代码填写。
    #[serde(default)]
    pub message: String,
    /// Tone of the icon, and of the title and outline unless they name their own.
    /// 图标的色调；标题与外框未另行指定时也使用此色调。
    #[serde(default = "default_tone")]
    pub tone: Tone,
    /// Tone of the title.
    /// 标题的色调。
    #[serde(default)]
    pub title_tone: Option<Tone>,
    /// Tone of the outline.
    /// 外框的色调。
    #[serde(default)]
    pub frame_tone: Option<Tone>,
    /// Anchor in the body.
    /// 主体中的锚点。
    #[serde(default)]
    pub placement: Placement,
    /// Outline style.
    /// 外框样式。
    #[serde(default)]
    pub frame: Frame,
    /// Shows a spinner instead of the tone icon while work is under way.
    /// 工作进行中时显示旋转指示而非色调图标。
    #[serde(default)]
    pub progress: bool,
    /// Lifetime of a pushed notice; `NOTICE_MS` when absent.
    /// 推送通知的存续时间；缺省为 `NOTICE_MS`。
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// Declarations are informational unless they name another tone.
/// 声明未指定其他色调时为信息色调。
fn default_tone() -> Tone {
    Tone::Info
}

impl NoticeSpec {
    /// The toast for a severity: info, warning or error.
    /// 某严重度的提示：信息、警告或错误。
    pub fn toast(severity: Severity, message: impl Into<String>) -> NoticeSpec {
        preset(
            match severity {
                Severity::Info => "toast_info",
                Severity::Warning => "toast_warning",
                Severity::Error => "toast_error",
            },
            message,
        )
    }
}

/// Built-in notice declarations, read once through the component parser.
/// 内置通知声明，经组件解析器读取一次。
fn presets() -> &'static BTreeMap<String, NoticeSpec> {
    static PRESETS: OnceLock<BTreeMap<String, NoticeSpec>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        let declarations: BTreeMap<String, Value> =
            serde_json::from_str(include_str!("notice-presets.json"))
                .expect("notice presets are a JSON object");
        declarations
            .into_iter()
            .map(|(name, declaration)| match Component::parse(declaration) {
                Ok(Component::Notice(spec)) => (name, spec),
                Err(error) => panic!("notice preset `{name}` is invalid: {error}"),
            })
            .collect()
    })
}

/// A built-in notice with its message.
/// 带消息的内置通知。
/// `name` is a key of `notice-presets.json`; a missing preset is a defect caught by the tests.
/// `name` 为 `notice-presets.json` 的键；缺失预设属于测试会发现的缺陷。
fn preset(name: &str, message: impl Into<String>) -> NoticeSpec {
    let mut spec = presets()
        .get(name)
        .unwrap_or_else(|| panic!("notice preset `{name}` is not declared"))
        .clone();
    spec.message = message.into();
    spec
}

/// Every component kind a declaration may name. Plugins will declare components in this same
/// JSON shape, tagged by `type`.
/// 声明可以使用的全部组件种类。插件日后将以相同的 JSON 形状声明组件，以 `type` 标记。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    Notice(NoticeSpec),
}

impl Component {
    /// Reads one declaration; unknown types and fields are refused with the reason.
    /// 读取一个声明；未知类型与字段被拒绝并附原因。
    pub fn parse(value: Value) -> Result<Component, String> {
        serde_json::from_value(value).map_err(|error| error.to_string())
    }
}

/// A pushed notice on screen until `until`.
/// 显示到 `until` 为止的推送通知。
#[derive(Debug, Clone)]
pub struct Notice {
    pub spec: NoticeSpec,
    pub until: Instant,
}

impl App {
    /// Shows one localized toast for a severity.
    /// 显示某严重度的一条本地化提示。
    pub fn toast(&mut self, severity: Severity, message: impl Into<String>) {
        self.notify(NoticeSpec::toast(severity, message));
    }

    /// Pushes a notice; a keyed notice replaces the one with its key.
    /// 推送一个通知；带键的通知替换同键通知。
    pub fn notify(&mut self, spec: NoticeSpec) {
        if let Some(key) = &spec.key {
            self.toasts
                .retain(|notice| notice.spec.key.as_ref() != Some(key));
        }
        let until = Instant::now() + Duration::from_millis(spec.duration_ms.unwrap_or(NOTICE_MS));
        self.toasts.push(Notice { spec, until });
        if self.toasts.len() > NOTICE_LIMIT {
            self.toasts.remove(0);
        }
    }
}

/// Notices derived from live state, shown while that state lasts, e.g. memory recall while the
/// open conversation is in its PreCheck phase (`runtime.memoryRecall` in the WebUI).
/// 由实时状态派生、在该状态持续期间显示的通知，例如当前会话处于 PreCheck 阶段时的记忆召回
/// （对应 WebUI 的 `runtime.memoryRecall`）。
pub fn live(app: &App) -> Vec<NoticeSpec> {
    let mut notices = Vec::new();
    if app.run_phase() == RunPhase::MemoryRecall {
        notices.push(preset("memory_recall", tr!("runtime.memoryRecall")));
    }
    notices
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Declarations fill presentation defaults and refuse unknown types or fields.
    /// 声明会补全呈现默认值，并拒绝未知类型或字段。
    #[test]
    fn component_declarations_parse_with_defaults_and_refuse_unknowns() {
        let Ok(Component::Notice(notice)) =
            Component::parse(json!({ "type": "notice", "message": "Saved" }))
        else {
            panic!("minimal notice parses");
        };
        assert_eq!(notice.message, "Saved");
        assert_eq!(notice.tone, Tone::Info);
        assert_eq!(notice.placement, Placement::TopRight);
        assert_eq!(notice.frame, Frame::Bar);
        assert!(notice.key.is_none() && notice.title.is_none() && !notice.progress);
        assert!(notice.duration_ms.is_none());

        let Ok(Component::Notice(notice)) = Component::parse(json!({
            "type": "notice",
            "key": "sync",
            "title": "Sync",
            "message": "Running",
            "tone": "accent",
            "title_tone": "success",
            "frame_tone": "warning",
            "placement": "bottom_left",
            "frame": "box",
            "progress": true,
            "duration_ms": 900,
        })) else {
            panic!("full notice parses");
        };
        assert_eq!(notice.placement, Placement::BottomLeft);
        assert_eq!(notice.frame, Frame::Box);
        assert_eq!(notice.title_tone, Some(Tone::Good));
        assert_eq!(notice.frame_tone, Some(Tone::Warn));
        assert!(notice.progress);

        assert!(Component::parse(json!({ "type": "dialog", "message": "x" })).is_err());
        assert!(
            Component::parse(json!({ "type": "notice", "message": "x", "color": "#fff" })).is_err()
        );
        assert!(
            Component::parse(json!({ "type": "notice", "message": "x", "tone": "pink" })).is_err()
        );
    }

    /// Every built-in notice the code shows is declared and valid.
    /// 代码显示的每个内置通知都已声明且有效。
    #[test]
    fn built_in_notices_are_declared() {
        for name in [
            "toast_info",
            "toast_warning",
            "toast_error",
            "memory_recall",
        ] {
            assert!(
                presets().contains_key(name),
                "notice preset `{name}` is used in code but missing from notice-presets.json"
            );
        }
        assert_eq!(NoticeSpec::toast(Severity::Error, "x").tone, Tone::Bad);
    }
}
