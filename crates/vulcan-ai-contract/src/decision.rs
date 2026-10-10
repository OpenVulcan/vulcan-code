//! Provider-neutral System One decision contract.
//! 供应商无关的 System One 决策契约。
//!
//! A decision request states a situation and asks closed questions about it; the answer to each
//! question is one of the options the question itself declares, with a probability per option.
//! 决策请求陈述一个情境并就其提出封闭问题；每个问题的回答是该问题自身声明的选项之一，并附每个选项
//! 的概率。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ExecutionRouteSnapshot, INFERENCE_CONTRACT_VERSION, RequestUsage};

/// Largest number of options one choice question may declare.
/// 单个选择题可声明的最多选项数。
pub const MAXIMUM_DECISION_CHOICE_OPTIONS: usize = 255;

/// One option of a choice question and the criterion that selects it.
/// 选择题的一个选项及选中它的判据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionCriterion {
    /// Option key the answer names.
    /// 回答所使用的选项键。
    pub option: String,
    /// Criterion text describing when this option applies.
    /// 描述该选项适用情形的判据文本。
    pub criterion: String,
}

/// One closed question about the stated situation.
/// 关于所陈述情境的一个封闭问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionQuestionKind {
    /// Pick exactly one of the declared options.
    /// 在声明的选项中恰好选择一个。
    Choice {
        /// Instructions the decision model follows.
        /// 决策模型遵循的指令。
        instructions: String,
        /// Options in the order they are presented.
        /// 按呈现顺序排列的选项。
        criteria: Vec<DecisionCriterion>,
    },
}

/// One identified question.
/// 一个带标识的问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionQuestion {
    /// Stable question identifier the answer is keyed by.
    /// 回答所使用的稳定问题标识。
    pub id: String,
    /// The question.
    /// 问题本身。
    #[serde(flatten)]
    pub kind: DecisionQuestionKind,
}

/// Carries one configured decision request.
/// 承载一次已配置决策请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    /// Exact contract version.
    /// 精确契约版本。
    pub contract_version: u32,
    /// Caller-generated stable request identifier.
    /// 调用方生成的稳定请求标识。
    pub request_id: String,
    /// Optional idempotency key for in-flight coalescing only.
    /// 仅用于运行中合并的可选幂等键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    /// Authorized consumer profile identifier.
    /// 已授权的消费者配置档标识。
    pub consumer_profile_id: String,
    /// The situation being decided, passed to the model unchanged.
    /// 待决策情境，原样交给模型。
    pub state: Value,
    /// Questions in presentation order.
    /// 按呈现顺序排列的问题。
    pub questions: Vec<DecisionQuestion>,
}

impl DecisionRequest {
    /// Validates identities, question identifiers and option sets.
    /// 校验身份、问题标识与选项集合。
    ///
    /// # Returns
    /// 返回值
    /// Success for one complete request, or a deterministic contract error.
    /// 完整请求有效时返回成功，否则返回确定性契约错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.contract_version != INFERENCE_CONTRACT_VERSION {
            return Err(format!(
                "Unsupported inference contract version {}; expected {INFERENCE_CONTRACT_VERSION}.",
                self.contract_version
            ));
        }
        if self.request_id.trim().is_empty() || self.consumer_profile_id.trim().is_empty() {
            return Err(String::from(
                "Decision request_id and consumer_profile_id must not be empty.",
            ));
        }
        if self
            .idempotency_key
            .as_deref()
            .is_some_and(|key| key.trim().is_empty())
        {
            return Err(String::from(
                "Decision idempotency_key must not be empty when present.",
            ));
        }
        if self.questions.is_empty() {
            return Err(String::from(
                "Decision request must ask at least one question.",
            ));
        }
        let mut ids = BTreeSet::new();
        for question in &self.questions {
            if question.id.trim().is_empty() || !ids.insert(question.id.as_str()) {
                return Err(format!(
                    "Decision question id `{}` is empty or repeated.",
                    question.id
                ));
            }
            let DecisionQuestionKind::Choice {
                instructions,
                criteria,
            } = &question.kind;
            if instructions.trim().is_empty() {
                return Err(format!(
                    "Decision question `{}` has no instructions.",
                    question.id
                ));
            }
            if criteria.len() < 2 || criteria.len() > MAXIMUM_DECISION_CHOICE_OPTIONS {
                return Err(format!(
                    "Decision question `{}` must declare 2 to {MAXIMUM_DECISION_CHOICE_OPTIONS} options.",
                    question.id
                ));
            }
            let mut options = BTreeSet::new();
            for criterion in criteria {
                if criterion.option.trim().is_empty()
                    || criterion.criterion.trim().is_empty()
                    || !options.insert(criterion.option.as_str())
                {
                    return Err(format!(
                        "Decision question `{}` has an empty or repeated option `{}`.",
                        question.id, criterion.option
                    ));
                }
            }
        }
        Ok(())
    }
}

/// The model's answer to one question.
/// 模型对一个问题的回答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionAnswer {
    /// The chosen option of a choice question.
    /// 选择题的所选选项。
    Choice {
        /// Chosen option key.
        /// 所选选项键。
        choice: String,
        /// Probability per declared option.
        /// 每个已声明选项的概率。
        probabilities: BTreeMap<String, f64>,
        /// The model's confidence in its choice.
        /// 模型对所选选项的置信度。
        confidence: f64,
    },
}

/// Carries one validated decision response.
/// 承载一次已校验决策响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResponse {
    /// Original request identifier.
    /// 原始请求标识。
    pub request_id: String,
    /// Immutable route snapshot.
    /// 不可变路由快照。
    pub route: ExecutionRouteSnapshot,
    /// Answers keyed by question identifier.
    /// 按问题标识索引的回答。
    pub answers: BTreeMap<String, DecisionAnswer>,
    /// Provider request usage.
    /// 供应商请求用量。
    pub usage: RequestUsage,
    /// End-to-end execution duration in milliseconds.
    /// 端到端执行耗时毫秒数。
    pub duration_ms: u64,
}

impl DecisionResponse {
    /// Validates that every question has exactly one answer within its declared options.
    /// 校验每个问题恰有一个回答，且回答落在其声明的选项内。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the immutable request that produced this response.
    /// `request` 是生成当前响应的不可变请求。
    ///
    /// # Returns
    /// 返回值
    /// Success only when answers and probabilities match the declared questions exactly.
    /// 仅当回答与概率与已声明问题完全一致时返回成功。
    pub fn validate_for_request(&self, request: &DecisionRequest) -> Result<(), String> {
        request.validate()?;
        if self.request_id != request.request_id {
            return Err(String::from(
                "Decision response request_id does not match the originating request.",
            ));
        }
        if self.answers.len() != request.questions.len() {
            return Err(format!(
                "Decision response answers {} questions; the request asked {}.",
                self.answers.len(),
                request.questions.len()
            ));
        }
        for question in &request.questions {
            let answer = self
                .answers
                .get(&question.id)
                .ok_or_else(|| format!("Decision response has no answer to `{}`.", question.id))?;
            let DecisionQuestionKind::Choice { criteria, .. } = &question.kind;
            let DecisionAnswer::Choice {
                choice,
                probabilities,
                confidence,
            } = answer;
            let declared = criteria
                .iter()
                .map(|criterion| criterion.option.as_str())
                .collect::<BTreeSet<_>>();
            if !declared.contains(choice.as_str()) {
                return Err(format!(
                    "Decision answer `{choice}` to `{}` is not a declared option.",
                    question.id
                ));
            }
            if probabilities
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != declared
            {
                return Err(format!(
                    "Decision probabilities for `{}` do not cover exactly the declared options.",
                    question.id
                ));
            }
            let unit = |value: f64| value.is_finite() && (0.0..=1.0).contains(&value);
            if !unit(*confidence) || !probabilities.values().all(|value| unit(*value)) {
                return Err(format!(
                    "Decision answer to `{}` has a probability outside 0 to 1.",
                    question.id
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> DecisionRequest {
        DecisionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("r"),
            idempotency_key: None,
            consumer_profile_id: String::from("permission_judge"),
            state: serde_json::json!({"call": "x"}),
            questions: vec![DecisionQuestion {
                id: String::from("decision"),
                kind: DecisionQuestionKind::Choice {
                    instructions: String::from("Judge."),
                    criteria: ["allow", "review"]
                        .into_iter()
                        .map(|option| DecisionCriterion {
                            option: option.to_string(),
                            criterion: format!("{option}."),
                        })
                        .collect(),
                },
            }],
        }
    }

    fn response(choice: &str, probabilities: &[(&str, f64)]) -> DecisionResponse {
        DecisionResponse {
            request_id: String::from("r"),
            route: serde_json::from_value(serde_json::json!({
                "provider_id": "p", "service_entry_id": "s", "connection_id": "c",
                "model_id": "m", "action_binding_id": "b", "capability_revision": "1",
                "catalog_revision": "1", "route_revision": "1",
                "usage_scope": "internal_background"
            }))
            .expect("route snapshot"),
            answers: BTreeMap::from([(
                String::from("decision"),
                DecisionAnswer::Choice {
                    choice: choice.to_string(),
                    probabilities: probabilities
                        .iter()
                        .map(|(key, value)| (key.to_string(), *value))
                        .collect(),
                    confidence: 0.5,
                },
            )]),
            usage: RequestUsage::default(),
            duration_ms: 1,
        }
    }

    #[test]
    fn requests_need_distinct_questions_and_options() {
        assert_eq!(request().validate(), Ok(()));
        let mut repeated = request();
        repeated.questions.push(repeated.questions[0].clone());
        assert!(repeated.validate().is_err());
        let mut single = request();
        let DecisionQuestionKind::Choice { criteria, .. } = &mut single.questions[0].kind;
        criteria.truncate(1);
        assert!(single.validate().is_err());
    }

    #[test]
    fn answers_must_stay_within_the_declared_options() {
        let request = request();
        let valid = response("allow", &[("allow", 0.9), ("review", 0.1)]);
        assert_eq!(valid.validate_for_request(&request), Ok(()));
        assert!(
            response("block", &[("allow", 0.9), ("review", 0.1)])
                .validate_for_request(&request)
                .is_err()
        );
        assert!(
            response("allow", &[("allow", 0.9)])
                .validate_for_request(&request)
                .is_err()
        );
        assert!(
            response("allow", &[("allow", 1.5), ("review", 0.1)])
                .validate_for_request(&request)
                .is_err()
        );
    }

    #[test]
    fn questions_serialize_with_a_type_tag() {
        let value = serde_json::to_value(&request().questions[0]).expect("question");
        assert_eq!(value["type"], "choice");
        assert_eq!(value["id"], "decision");
        assert_eq!(value["criteria"][0]["option"], "allow");
    }
}
