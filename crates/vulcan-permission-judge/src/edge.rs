//! Official direct judge: the wire shape of an edge endpoint's `POST /v1/judge` and the checks its
//! answer must pass.
//! 官方直连判定：边缘端点 `POST /v1/judge` 的线协议形状及其回答必须通过的检查。
//!
//! The edge endpoint runs an official package on a GPU node and applies the package's frozen margin
//! rule there, so its label stands as is. The client renders the input with the policy this build
//! ships, the same one the cloud judge reads, and refuses an answer from another checkpoint or
//! policy.
//! 边缘端点在 GPU 节点上运行官方模型包，并在节点上应用模型包冻结的余量规则，因此其标签原样采用。
//! 客户端用本构建随附的策略（与云端判定读取的相同）渲染输入，并拒绝来自其他检查点或策略的回答。

use serde::{Deserialize, Serialize};

use crate::protocol::JudgeInput;
use crate::{JudgeLabel, KnownPackage};

/// Path of the judge route below an edge endpoint's base URL.
/// 判定路由在边缘端点基础地址下的路径。
pub const EDGE_JUDGE_PATH: &str = "/v1/judge";

/// Reason code of a result whose input exceeded the model's token limit.
/// 输入超过模型 token 上限的结果所带的原因代码。
const INPUT_TOKEN_LIMIT_REASON: &str = "input_token_limit";

/// Body of `POST /v1/judge`.
/// `POST /v1/judge` 的请求体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdgeJudgeRequest {
    /// Caller-chosen identifier of 1–64 characters, echoed in the answer.
    /// 调用方选择的 1–64 字符标识，回答中原样返回。
    pub request_id: String,
    /// Calls to judge, answered one result each in order.
    /// 待判定的调用，按顺序各得一条结果。
    pub inputs: Vec<JudgeInput>,
}

/// Answer of `POST /v1/judge`.
/// `POST /v1/judge` 的回答。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EdgeJudgeResponse {
    /// Echo of the request identifier.
    /// 请求标识的回显。
    pub request_id: String,
    /// Region of the endpoint that answered.
    /// 作答端点所在的区域。
    pub region: String,
    /// Model id the endpoint serves.
    /// 端点提供的模型 id。
    pub model: String,
    /// Digest of the checkpoint the node ran.
    /// 节点运行的检查点摘要。
    pub checkpoint_sha256: String,
    /// Policy revision of the deployed package.
    /// 已部署模型包的策略修订。
    pub policy_version: String,
    /// One result per input, in order.
    /// 每条输入一条结果，按顺序排列。
    pub results: Vec<EdgeJudgeResult>,
}

/// Result for one input.
/// 单条输入的结果。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EdgeJudgeResult {
    /// Decision after the package's margin rule.
    /// 应用模型包余量规则后的判定。
    pub label: JudgeLabel,
    /// Raw scores in label order; present when the model ran.
    /// 按标签顺序排列的原始分数；模型运行时存在。
    #[serde(default)]
    pub logits: Option<[f32; 3]>,
    /// Sequence length the model read; present when the model ran.
    /// 模型读取的序列长度；模型运行时存在。
    #[serde(default)]
    pub input_tokens: Option<u32>,
    /// Whether the model scored this input.
    /// 模型是否为该输入评分。
    pub model_evaluated: bool,
    /// Why the model did not run, when it did not.
    /// 模型未运行时的原因。
    #[serde(default)]
    pub reason_code: Option<String>,
}

/// One checked edge result.
/// 一条已检查的边缘结果。
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeOutcome {
    /// The model scored the call.
    /// 模型已为调用评分。
    Evaluated {
        /// Decision after the package's margin rule.
        /// 应用模型包余量规则后的判定。
        label: JudgeLabel,
        /// Raw scores in label order.
        /// 按标签顺序排列的原始分数。
        logits: [f32; 3],
        /// Sequence length the model read.
        /// 模型读取的序列长度。
        input_tokens: u32,
    },
    /// The call exceeds the model's token limit; the model did not run.
    /// 调用超过模型 token 上限；模型未运行。
    InputTokenLimit,
}

/// Checks an answer to a request of `expected` inputs: it must come from a checkpoint of `package`,
/// use `policy_version`, echo `request_id` and hold one well-formed result per input.
/// 检查含 `expected` 条输入的请求的回答：它必须来自 `package` 的检查点、使用 `policy_version`、回显
/// `request_id`，并且每条输入恰有一条格式正确的结果。
pub fn edge_outcomes(
    package: &KnownPackage,
    policy_version: &str,
    request_id: &str,
    expected: usize,
    response: EdgeJudgeResponse,
) -> Result<Vec<EdgeOutcome>, String> {
    if response.request_id != request_id {
        return Err(String::from("The edge judge answered a different request."));
    }
    if !package.checkpoints.contains(&response.checkpoint_sha256) {
        return Err(format!(
            "The edge judge ran checkpoint `{}`, which is not part of {}.",
            response.checkpoint_sha256, package.display_name
        ));
    }
    if response.policy_version != policy_version {
        return Err(format!(
            "The edge judge uses policy `{}`, not `{policy_version}`.",
            response.policy_version
        ));
    }
    if response.results.len() != expected {
        return Err(format!(
            "The edge judge returned {} results for {expected} calls.",
            response.results.len()
        ));
    }
    response.results.into_iter().map(edge_result).collect()
}

/// Checks one result: a scored result carries finite logits and its token count, and an unscored
/// one is the token-limit block.
/// 检查一条结果：已评分结果带有有限分数与 token 数，未评分结果只能是 token 上限拦截。
fn edge_result(result: EdgeJudgeResult) -> Result<EdgeOutcome, String> {
    match result {
        EdgeJudgeResult {
            label,
            logits: Some(logits),
            input_tokens: Some(input_tokens),
            model_evaluated: true,
            reason_code: None,
        } if logits.iter().all(|score| score.is_finite()) => Ok(EdgeOutcome::Evaluated {
            label,
            logits,
            input_tokens,
        }),
        EdgeJudgeResult {
            label: JudgeLabel::Block,
            model_evaluated: false,
            reason_code: Some(reason),
            ..
        } if reason == INPUT_TOKEN_LIMIT_REASON => Ok(EdgeOutcome::InputTokenLimit),
        _ => Err(String::from("The edge judge returned a malformed result.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package() -> KnownPackage {
        KnownPackage {
            id: String::from("a"),
            display_name: String::from("A"),
            checkpoints: vec!["a".repeat(64)],
            edge_model_id: Some(String::from("shell-judge")),
            variants: Vec::new(),
        }
    }

    fn response(results: serde_json::Value) -> EdgeJudgeResponse {
        serde_json::from_value(serde_json::json!({
            "request_id": "r1", "region": "hk", "model": "shell-judge",
            "checkpoint_sha256": "a".repeat(64), "policy_version": "p1", "results": results
        }))
        .unwrap()
    }

    #[test]
    fn evaluated_and_token_limit_results_are_accepted() {
        let scored = response(serde_json::json!([
            {"label": "allow", "logits": [4.0, -2.0, -2.0], "input_tokens": 37, "model_evaluated": true}
        ]));
        assert_eq!(
            edge_outcomes(&package(), "p1", "r1", 1, scored),
            Ok(vec![EdgeOutcome::Evaluated {
                label: JudgeLabel::Allow,
                logits: [4.0, -2.0, -2.0],
                input_tokens: 37
            }])
        );
        let over = response(serde_json::json!([
            {"label": "block", "reason_code": "input_token_limit", "model_evaluated": false}
        ]));
        assert_eq!(
            edge_outcomes(&package(), "p1", "r1", 1, over),
            Ok(vec![EdgeOutcome::InputTokenLimit])
        );
    }

    #[test]
    fn foreign_checkpoints_policies_requests_and_malformed_results_are_refused() {
        let scored = || {
            response(serde_json::json!([
                {"label": "allow", "logits": [4.0, -2.0, -2.0], "input_tokens": 37, "model_evaluated": true}
            ]))
        };
        let foreign = EdgeJudgeResponse {
            checkpoint_sha256: "b".repeat(64),
            ..scored()
        };
        assert!(edge_outcomes(&package(), "p1", "r1", 1, foreign).is_err());
        assert!(edge_outcomes(&package(), "p2", "r1", 1, scored()).is_err());
        assert!(edge_outcomes(&package(), "p1", "r2", 1, scored()).is_err());
        for results in [
            serde_json::json!([]),
            serde_json::json!([
                {"label": "allow", "logits": [1.0, 0.0, 0.0], "input_tokens": 1, "model_evaluated": true},
                {"label": "allow", "logits": [1.0, 0.0, 0.0], "input_tokens": 1, "model_evaluated": true}
            ]),
            serde_json::json!([{"label": "allow", "input_tokens": 1, "model_evaluated": true}]),
            serde_json::json!([{"label": "allow", "reason_code": "input_token_limit", "model_evaluated": false}]),
            serde_json::json!([{"label": "block", "reason_code": "other", "model_evaluated": false}]),
        ] {
            assert!(
                edge_outcomes(&package(), "p1", "r1", 1, response(results.clone())).is_err(),
                "{results}"
            );
        }
    }

    #[test]
    fn request_serializes_to_the_endpoint_shape() {
        let request = EdgeJudgeRequest {
            request_id: String::from("r1"),
            inputs: vec![JudgeInput {
                settings: [(String::from("project_reset"), String::from("confirm"))].into(),
                rendered: String::from("Tool: shell_command"),
            }],
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"request_id": "r1", "inputs": [
                {"settings": {"project_reset": "confirm"}, "rendered": "Tool: shell_command"}
            ]})
        );
    }
}
