//! Batch retention for settled user turns in economy mode.
//! 节约用量模式下已结算用户回合的批量保留策略。

use serde::{Deserialize, Serialize};

/// Counts complete user turns in the provider history window.
/// 统计供应商历史窗口中的完整用户回合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EconomyWindow {
    /// Start trimming once the settled history reaches this count.
    /// 已结算历史达到此轮数时开始裁剪。
    pub trigger_turns: u32,
    /// Number of oldest complete turns removed per batch.
    /// 每批移出的最旧完整回合数。
    pub batch_turns: u32,
}

impl Default for EconomyWindow {
    fn default() -> Self {
        Self {
            trigger_turns: 40,
            batch_turns: 20,
        }
    }
}

impl EconomyWindow {
    /// Rejects empty batches and policies that could remove the entire window.
    /// 拒绝空批次以及可能移出整个窗口的配置。
    pub fn validate(self) -> Result<(), String> {
        if self.batch_turns == 0 || self.batch_turns >= self.trigger_turns {
            return Err(String::from(
                "The batch size must be positive and smaller than the trigger count.",
            ));
        }
        Ok(())
    }

    /// Returns the oldest complete batch count to remove from the given settled length.
    /// 根据已结算长度返回本次应移出的全部完整批次所占轮数。
    pub fn turns_to_remove(self, settled: usize) -> Result<usize, String> {
        self.validate()?;
        let trigger = self.trigger_turns as usize;
        let batch = self.batch_turns as usize;
        Ok(if settled < trigger {
            0
        } else {
            ((settled - trigger) / batch + 1) * batch
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises a custom window at threshold, between trims and after lowering the threshold.
    /// 验证自定义窗口在达到阈值、两次裁剪之间以及降低阈值后的行为。
    #[test]
    fn trims_whole_batches_only() {
        let window = EconomyWindow {
            trigger_turns: 18,
            batch_turns: 6,
        };
        assert_eq!(window.turns_to_remove(17).unwrap(), 0);
        assert_eq!(window.turns_to_remove(18).unwrap(), 6);
        assert_eq!(window.turns_to_remove(34).unwrap(), 18);
        assert!(
            EconomyWindow {
                trigger_turns: 6,
                batch_turns: 6
            }
            .validate()
            .is_err()
        );
        assert!(
            EconomyWindow {
                trigger_turns: 18,
                batch_turns: 0
            }
            .validate()
            .is_err()
        );
    }
}
