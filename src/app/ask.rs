// Answer state for the non-modal `ask` panel.
// 非模态 `ask` 面板的作答状态。
//
// The panel lives inside the conversation column and only receives keys while it has focus; it
// never captures input from other regions. Answers are sent as `string[][]`, one list per question.
// 面板位于对话列内，只在获得焦点时接收按键，从不劫持其他区域的输入。回答以 `string[][]` 发送，
// 每个问题一组。

use std::collections::HashMap;

use crate::model::PendingQuestion;

#[derive(Debug, Default, Clone)]
pub struct AskState {
    pub index: usize,
    /// Option cursor; `options.len()` is the free-form "other answer" row.
    /// 选项光标；`options.len()` 表示自由填写的“其他答案”行。
    pub cursor: usize,
    pub selections: HashMap<usize, Vec<usize>>,
    pub other: HashMap<usize, String>,
    pub editing_other: bool,
    pub confirm_reject: bool,
    /// Index of the first unanswered question after a failed submit.
    /// 提交失败后第一个未作答问题的序号。
    pub missing: Option<usize>,
}

impl AskState {
    pub fn is_selected(&self, question: usize, option: usize) -> bool {
        self.selections
            .get(&question)
            .is_some_and(|s| s.contains(&option))
    }

    /// Toggles (multiple) or sets (single) one option of the current question.
    /// 切换（多选）或设置（单选）当前问题的一个选项。
    pub fn choose(&mut self, option: usize, multiple: bool) {
        let entry = self.selections.entry(self.index).or_default();
        if multiple {
            if let Some(pos) = entry.iter().position(|o| *o == option) {
                entry.remove(pos);
            } else {
                entry.push(option);
                entry.sort_unstable();
            }
        } else {
            *entry = vec![option];
            self.other.remove(&self.index);
        }
        self.missing = None;
    }

    pub fn other_text(&self, question: usize) -> &str {
        self.other.get(&question).map_or("", String::as_str)
    }

    pub fn other_mut(&mut self, multiple: bool) -> &mut String {
        if !multiple {
            self.selections.remove(&self.index);
        }
        self.missing = None;
        self.other.entry(self.index).or_default()
    }

    pub fn go_to(&mut self, index: usize, total: usize) {
        self.index = index.min(total.saturating_sub(1));
        self.cursor = self
            .selections
            .get(&self.index)
            .and_then(|s| s.first().copied())
            .unwrap_or(0);
        self.editing_other = false;
    }

    /// Builds the answer payload, or returns the first unanswered question index.
    /// 构建回答载荷；有未作答问题时返回其序号。
    pub fn answers(&self, pending: &PendingQuestion) -> Result<Vec<Vec<String>>, usize> {
        pending
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                let mut answer: Vec<String> = self
                    .selections
                    .get(&index)
                    .map(|chosen| {
                        chosen
                            .iter()
                            .filter_map(|o| question.options.get(*o))
                            .map(|o| o.label.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                let other = self.other_text(index).trim();
                if !other.is_empty() {
                    answer.push(other.to_string());
                }
                if answer.is_empty() {
                    Err(index)
                } else {
                    Ok(answer)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Question, QuestionOption};

    fn pending() -> PendingQuestion {
        let option = |label: &str| QuestionOption {
            label: label.into(),
            description: String::new(),
        };
        PendingQuestion {
            id: "q".into(),
            questions: vec![
                Question {
                    header: "A".into(),
                    question: "single".into(),
                    options: vec![option("x"), option("y")],
                    multiple: false,
                },
                Question {
                    header: "B".into(),
                    question: "multi".into(),
                    options: vec![option("m"), option("n")],
                    multiple: true,
                },
            ],
        }
    }

    #[test]
    fn answers_require_every_question() {
        let mut state = AskState::default();
        state.choose(1, false);
        assert_eq!(state.answers(&pending()), Err(1));
        state.go_to(1, 2);
        state.choose(0, true);
        state.choose(1, true);
        state.other_mut(true).push_str("custom");
        assert_eq!(
            state.answers(&pending()),
            Ok(vec![
                vec!["y".to_string()],
                vec!["m".to_string(), "n".to_string(), "custom".to_string()]
            ])
        );
    }
}
