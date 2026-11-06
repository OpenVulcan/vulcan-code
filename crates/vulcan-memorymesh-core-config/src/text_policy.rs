//! Configurable PostAction text reduction, independent of mandatory privacy and reasoning cleanup.
//! 可配置的 PostAction 文本缩减，独立于强制隐私与思考内容清理。

/// Freezes text reduction choices for one newly admitted intake.
/// 为一次新准入冻结文本缩减选择。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostActionTextPolicy {
    /// Enables machine-block folding without disabling media, thought, PII or noise handling.
    /// 启用机器文本块折叠，不关闭媒体、思考内容、PII 或噪音处理。
    pub fold_machine_text: bool,
    /// Approximate per-fragment token target, distinct from the later L1 target-plus-history budget.
    /// 每个片段的近似 Token 目标，区别于后续 L1 目标回合加历史的预算。
    pub max_fragment_tokens: u32,
}

/// Default per-fragment token target shared by the PostAction policy and default clipping.
/// PostAction 策略与默认裁剪共用的默认单片段 Token 目标。
pub const DEFAULT_MAX_FRAGMENT_TOKENS: u32 = 1200;

impl Default for PostActionTextPolicy {
    /// Returns the already differential-tested external default behavior.
    /// 返回已经差异对照验证的外置默认行为。
    fn default() -> Self {
        Self {
            fold_machine_text: true,
            max_fragment_tokens: DEFAULT_MAX_FRAGMENT_TOKENS,
        }
    }
}

impl PostActionTextPolicy {
    /// Checks the bounded token target before admission; returns the exact invalid-policy error.
    /// 准入前检查有界 Token 目标，返回准确的非法策略错误。
    pub fn validate(&self) -> Result<(), String> {
        if !(256..=131_072).contains(&self.max_fragment_tokens) {
            return Err(
                "PostAction fragment token target must be between 256 and 131072".to_owned(),
            );
        }
        Ok(())
    }
}
