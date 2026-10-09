//! Decision rule applied to the three choice logits.
//! 应用于三个选项分数的判定规则。

use serde::{Deserialize, Serialize};

/// One judge decision, in the trained classifier order.
/// 一个判定结果，按训练分类器的顺序排列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeLabel {
    /// Ordinary work, or an effect the selected options permit.
    /// 普通工作，或所选选项允许的效果。
    Allow,
    /// A concrete serious effect that needs confirmation.
    /// 需要确认的具体严重效果。
    Review,
    /// Direct extreme damage, exfiltration or broad security disabling.
    /// 直接极端破坏、外泄或大范围关闭安全机制。
    Block,
}

impl JudgeLabel {
    /// Labels in logit order; packages must declare exactly these keys in this order.
    /// 按分数顺序排列的标签；模型包必须按此顺序声明这些键。
    pub const ORDER: [JudgeLabel; 3] = [JudgeLabel::Allow, JudgeLabel::Review, JudgeLabel::Block];

    /// Canonical key used by packages and the wire protocol.
    /// 模型包与线协议使用的规范键。
    pub fn key(self) -> &'static str {
        match self {
            JudgeLabel::Allow => "allow",
            JudgeLabel::Review => "review",
            JudgeLabel::Block => "block",
        }
    }
}

/// The model produced a NaN or infinite score.
/// 模型产生了 NaN 或无穷分数。
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("The judge model produced a non-finite score.")]
pub struct NonFiniteLogits;

/// Applies the frozen margin rule: the first maximum wins, but allow needs `allow_margin` over the
/// best other score or it becomes review.
/// 应用冻结的余量规则：首个最大值胜出，但 allow 必须比其他最高分高出 `allow_margin`，否则改为 review。
pub fn decide(logits: [f32; 3], allow_margin: f32) -> Result<JudgeLabel, NonFiniteLogits> {
    if logits.iter().any(|score| !score.is_finite()) {
        return Err(NonFiniteLogits);
    }
    let mut choice = 0;
    for (index, score) in logits.iter().enumerate() {
        if *score > logits[choice] {
            choice = index;
        }
    }
    if choice == 0 && logits[0] - logits[1].max(logits[2]) < allow_margin {
        choice = 1;
    }
    Ok(JudgeLabel::ORDER[choice])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_needs_the_margin_and_ties_keep_the_first_label() {
        assert_eq!(decide([4.35, -2.13, -2.14], 1.0), Ok(JudgeLabel::Allow));
        assert_eq!(decide([1.5, 0.6, -1.0], 1.0), Ok(JudgeLabel::Review));
        assert_eq!(decide([-2.1, 4.3, -2.1], 1.0), Ok(JudgeLabel::Review));
        assert_eq!(decide([0.0, 1.0, 1.0], 1.0), Ok(JudgeLabel::Review));
        assert_eq!(decide([0.0, -1.0, 2.0], 1.0), Ok(JudgeLabel::Block));
        assert_eq!(decide([f32::NAN, 0.0, 0.0], 1.0), Err(NonFiniteLogits));
    }
}
