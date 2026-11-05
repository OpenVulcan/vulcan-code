//! Provider-neutral context-compaction window identities.
//! 供应商无关的上下文压缩窗口身份契约。

use serde::{Deserialize, Serialize};

/// Identifies one fixed system-owned context-compaction window node.
/// 标识一个由系统持有的固定上下文压缩窗口节点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ContextCompactionWindowNodeId {
    /// First and smallest configurable compaction-window node.
    /// 第一个且最小的可配置压缩窗口节点。
    #[serde(rename = "compression-window-1")]
    CompressionWindow1,
    /// Second configurable compaction-window node.
    /// 第二个可配置压缩窗口节点。
    #[serde(rename = "compression-window-2")]
    CompressionWindow2,
    /// Third configurable compaction-window node.
    /// 第三个可配置压缩窗口节点。
    #[serde(rename = "compression-window-3")]
    CompressionWindow3,
    /// Fourth configurable compaction-window node.
    /// 第四个可配置压缩窗口节点。
    #[serde(rename = "compression-window-4")]
    CompressionWindow4,
    /// Fifth and largest configurable compaction-window node.
    /// 第五个且最大的可配置压缩窗口节点。
    #[serde(rename = "compression-window-5")]
    CompressionWindow5,
}

impl ContextCompactionWindowNodeId {
    /// Lists every supported node in the immutable product order.
    /// 按不可变产品顺序列出全部受支持节点。
    pub const ALL: [Self; 5] = [
        Self::CompressionWindow1,
        Self::CompressionWindow2,
        Self::CompressionWindow3,
        Self::CompressionWindow4,
        Self::CompressionWindow5,
    ];

    /// Returns the zero-based immutable order index for this node.
    /// 返回当前节点从零开始的不可变顺序索引。
    pub const fn index(self) -> usize {
        match self {
            Self::CompressionWindow1 => 0,
            Self::CompressionWindow2 => 1,
            Self::CompressionWindow3 => 2,
            Self::CompressionWindow4 => 3,
            Self::CompressionWindow5 => 4,
        }
    }

    /// Returns the stable serialized identifier used by sessions and WebUI projections.
    /// 返回会话与 WebUI 投影使用的稳定序列化标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompressionWindow1 => "compression-window-1",
            Self::CompressionWindow2 => "compression-window-2",
            Self::CompressionWindow3 => "compression-window-3",
            Self::CompressionWindow4 => "compression-window-4",
            Self::CompressionWindow5 => "compression-window-5",
        }
    }
}

impl std::str::FromStr for ContextCompactionWindowNodeId {
    type Err = String;

    /// Parses one exact stable node identifier without aliases or fallback values.
    /// 解析一个精确稳定节点标识，不接受别名或回退值。
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "compression-window-1" => Ok(Self::CompressionWindow1),
            "compression-window-2" => Ok(Self::CompressionWindow2),
            "compression-window-3" => Ok(Self::CompressionWindow3),
            "compression-window-4" => Ok(Self::CompressionWindow4),
            "compression-window-5" => Ok(Self::CompressionWindow5),
            unsupported => Err(format!(
                "Unsupported context compaction window node id `{unsupported}`."
            )),
        }
    }
}

/// Smallest token count accepted for one custom context window.
/// 自定义上下文窗口允许的最小 Token 数。
pub const CUSTOM_CONTEXT_WINDOW_MIN_TOKENS: u64 = 100_000;

/// Largest integer every client can represent exactly (JavaScript `Number.MAX_SAFE_INTEGER`).
/// 所有客户端都能精确表示的最大整数（JavaScript `Number.MAX_SAFE_INTEGER`）。
pub const CONTEXT_WINDOW_MAX_SAFE_TOKENS: u64 = 9_007_199_254_740_991;

/// One explicit context-window choice: a fixed system node or a custom token count.
/// 一个显式上下文窗口选择：固定系统节点或自定义 Token 数。
///
/// Absence of a selection means the session follows the model's own window.
/// 没有选择表示会话跟随模型自身窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextWindowSelection {
    /// A fixed node whose token value follows later edits of that node.
    /// 固定节点，其 Token 值随该节点后续修改而变化。
    Node(ContextCompactionWindowNodeId),
    /// An exact custom token count.
    /// 精确的自定义 Token 数。
    Custom(u64),
}

impl ContextWindowSelection {
    /// Returns the stable diagnostic label used by fingerprints and error messages.
    /// 返回指纹与错误消息使用的稳定诊断标签。
    pub fn label(self) -> String {
        match self {
            Self::Node(node_id) => node_id.as_str().to_owned(),
            Self::Custom(tokens) => format!("custom-{tokens}"),
        }
    }

    /// Validates the value range of a custom selection; node selections are always valid here.
    /// 校验自定义选择的取值范围；节点选择在此处总是有效。
    pub fn validate(self) -> Result<(), String> {
        match self {
            Self::Node(_) => Ok(()),
            Self::Custom(tokens) if tokens < CUSTOM_CONTEXT_WINDOW_MIN_TOKENS => Err(format!(
                "A custom context window must be at least {CUSTOM_CONTEXT_WINDOW_MIN_TOKENS} tokens; got {tokens}."
            )),
            Self::Custom(tokens) if tokens > CONTEXT_WINDOW_MAX_SAFE_TOKENS => Err(format!(
                "A custom context window must not exceed {CONTEXT_WINDOW_MAX_SAFE_TOKENS} tokens; got {tokens}."
            )),
            Self::Custom(_) => Ok(()),
        }
    }
}

/// Reports whether a window of `tokens` can be used with a model whose window is `model_limit`.
/// 报告 `tokens` 大小的窗口能否用于窗口为 `model_limit` 的模型。
///
/// This is the single rule for every node, custom value, default, and model switch; a window
/// that does not fit falls back to following the model.
/// 这是所有节点、自定义值、默认值与模型切换共用的唯一规则；放不下的窗口回退为跟随模型。
pub const fn context_window_fits_model(tokens: u64, model_limit: u64) -> bool {
    tokens <= model_limit
}

/// Parses typed custom-window input: a plain integer, or an integer followed by `K` where 1K = 1000.
/// 解析输入的自定义窗口：纯整数，或后接 `K` 的整数，其中 1K = 1000。
///
/// Surrounding whitespace and whitespace before `K` are ignored. Returns `None` for any other
/// shape, zero, or a value above [`CONTEXT_WINDOW_MAX_SAFE_TOKENS`]; range checks against the
/// minimum and the model window stay with [`ContextWindowSelection::validate`] and
/// [`context_window_fits_model`].
/// 忽略首尾空白及 `K` 前的空白。其他形状、零或超过 [`CONTEXT_WINDOW_MAX_SAFE_TOKENS`] 的值返回
/// `None`；下限与模型窗口的范围校验仍由 [`ContextWindowSelection::validate`] 与
/// [`context_window_fits_model`] 负责。
pub fn parse_custom_context_window_tokens(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, multiplier) = match text.strip_suffix(['k', 'K']) {
        Some(digits) => (digits.trim_end(), 1_000),
        None => (text, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let tokens = digits.parse::<u64>().ok()?.checked_mul(multiplier)?;
    (tokens > 0 && tokens <= CONTEXT_WINDOW_MAX_SAFE_TOKENS).then_some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Selections use one-word externally tagged JSON so every surface reads the same shape.
    /// 选择使用单词外部标签 JSON，所有界面读取同一形状。
    #[test]
    fn context_window_selection_json_shape() {
        let node = ContextWindowSelection::Node(ContextCompactionWindowNodeId::CompressionWindow3);
        let custom = ContextWindowSelection::Custom(235_000);
        assert_eq!(
            serde_json::to_value(node).ok(),
            Some(serde_json::json!({"node": "compression-window-3"}))
        );
        assert_eq!(
            serde_json::to_value(custom).ok(),
            Some(serde_json::json!({"custom": 235_000}))
        );
        assert_eq!(
            serde_json::from_value::<ContextWindowSelection>(
                serde_json::json!({"custom": 235_000})
            )
            .ok(),
            Some(custom)
        );
        assert!(
            serde_json::from_value::<ContextWindowSelection>(serde_json::json!({"node": "x"}))
                .is_err()
        );
    }

    /// Custom values below the minimum or above the safe integer range are rejected.
    /// 低于最小值或超出安全整数范围的自定义值被拒绝。
    #[test]
    fn custom_context_window_range() {
        assert!(
            ContextWindowSelection::Custom(CUSTOM_CONTEXT_WINDOW_MIN_TOKENS)
                .validate()
                .is_ok()
        );
        assert!(
            ContextWindowSelection::Custom(CUSTOM_CONTEXT_WINDOW_MIN_TOKENS - 1)
                .validate()
                .is_err()
        );
        assert!(
            ContextWindowSelection::Custom(CONTEXT_WINDOW_MAX_SAFE_TOKENS + 1)
                .validate()
                .is_err()
        );
        assert!(context_window_fits_model(200_000, 200_000));
        assert!(!context_window_fits_model(200_001, 200_000));
    }

    /// Custom input accepts plain integers and integer K values only, with 1K = 1000.
    /// 自定义输入只接受纯整数与整数 K 值，其中 1K = 1000。
    #[test]
    fn custom_context_window_input_shapes() {
        assert_eq!(parse_custom_context_window_tokens("235000"), Some(235_000));
        assert_eq!(parse_custom_context_window_tokens(" 235K "), Some(235_000));
        assert_eq!(parse_custom_context_window_tokens("358 k"), Some(358_000));
        for text in [
            "",
            "K",
            "1.5K",
            "235M",
            "-5",
            "12,000",
            "0",
            "99999999999999999999",
        ] {
            assert_eq!(parse_custom_context_window_tokens(text), None, "{text}");
        }
    }

    /// Stable node identities round-trip without aliases.
    /// 稳定节点身份不经过别名即可往返。
    #[test]
    fn context_compaction_window_node_ids_round_trip() {
        for node in ContextCompactionWindowNodeId::ALL {
            assert_eq!(
                node.as_str().parse::<ContextCompactionWindowNodeId>(),
                Ok(node)
            );
            let expected = format!("\"{}\"", node.as_str());
            assert_eq!(
                serde_json::to_string(&node).ok().as_deref(),
                Some(expected.as_str())
            );
        }
        assert!(
            "compression-window-6"
                .parse::<ContextCompactionWindowNodeId>()
                .is_err()
        );
    }
}
