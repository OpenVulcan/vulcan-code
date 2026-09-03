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

#[cfg(test)]
mod tests {
    use super::*;

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
