//! Cloud judge: the wire shape of an edge endpoint's `POST /v1/judge` and the checks its
//! answer must pass.
//! 云端判定：边缘端点 `POST /v1/judge` 的线协议形状及其回答必须通过的检查。
//!
//! The edge endpoint runs an official model on a GPU node and applies its frozen margin rule there,
//! so its label stands as is. The operator chooses which models and checkpoints are served, so the
//! client does not pin a checkpoint: it records the one the node reports. The client renders the
//! input with the policy this build ships, the same one the API judge reads, and refuses an
//! answer under another policy.
//! 边缘端点在 GPU 节点上运行官方模型，并在节点上应用其冻结的余量规则，因此其标签原样采用。提供哪些
//! 模型与检查点由运营方决定，客户端不固定检查点，只记录节点报告的检查点。客户端用本构建随附的策略
//! （与 API 判定读取的相同）渲染输入，并拒绝其他策略下的回答。

use serde::{Deserialize, Serialize};

use crate::JudgeLabel;
use crate::known::is_sha256;
use crate::protocol::JudgeInput;

/// Path of the judge route below an edge endpoint's base URL.
/// 判定路由在边缘端点基础地址下的路径。
pub const EDGE_JUDGE_PATH: &str = "/v1/judge";

/// Header in which an edge endpoint reports its stage times.
/// 边缘端点报告各阶段耗时的响应头。
pub const EDGE_SERVER_TIMING_HEADER: &str = "server-timing";

/// Stage times of one answer, in milliseconds, from `Server-Timing: edge;dur=…, node;dur=…,
/// queue;dur=…, compute;dur=…`; a stage the endpoint did not report is `None`.
/// 一次回答的各阶段耗时（毫秒），取自 `Server-Timing: edge;dur=…, node;dur=…, queue;dur=…,
/// compute;dur=…`；端点未报告的阶段为 `None`。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeServerTiming {
    /// Time inside the endpoint.
    /// 端点内部耗时。
    pub edge: Option<f64>,
    /// Round trip between the endpoint and its node.
    /// 端点与其节点之间的往返耗时。
    pub node: Option<f64>,
    /// Time the inputs waited in the node's queue.
    /// 输入在节点队列中的等待耗时。
    pub queue: Option<f64>,
    /// Model compute time on the node.
    /// 节点上的模型计算耗时。
    pub compute: Option<f64>,
}

impl EdgeServerTiming {
    /// Reads the header value; unknown metrics and malformed or negative durations are ignored.
    /// 读取响应头取值；忽略未知指标以及格式不符或为负的耗时。
    pub fn parse(header: &str) -> Self {
        let mut timing = Self::default();
        for metric in header.split(',') {
            let mut parts = metric.split(';').map(str::trim);
            let name = parts.next().unwrap_or_default();
            let duration = parts
                .filter_map(|parameter| parameter.strip_prefix("dur="))
                .find_map(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0);
            let slot = match name {
                "edge" => &mut timing.edge,
                "node" => &mut timing.node,
                "queue" => &mut timing.queue,
                "compute" => &mut timing.compute,
                _ => continue,
            };
            *slot = duration;
        }
        timing
    }
}

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

/// A checked edge answer.
/// 一次已检查的边缘回答。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeAnswer {
    /// Digest of the checkpoint the node reported, kept in the decision trace.
    /// 节点报告的检查点摘要，保存在判定轨迹中。
    pub checkpoint_sha256: String,
    /// One outcome per input, in order.
    /// 每条输入一条结果，按顺序排列。
    pub outcomes: Vec<EdgeOutcome>,
}

/// Checks an answer to a request of `expected` inputs: it must echo `request_id`, report a
/// well-formed checkpoint digest, use `policy_version` and hold one well-formed result per input.
/// 检查含 `expected` 条输入的请求的回答：它必须回显 `request_id`、报告格式正确的检查点摘要、使用
/// `policy_version`，并且每条输入恰有一条格式正确的结果。
pub fn edge_outcomes(
    policy_version: &str,
    request_id: &str,
    expected: usize,
    response: EdgeJudgeResponse,
) -> Result<EdgeAnswer, String> {
    if response.request_id != request_id {
        return Err(String::from("The edge judge answered a different request."));
    }
    if !is_sha256(&response.checkpoint_sha256) {
        return Err(String::from(
            "The edge judge reported a malformed checkpoint digest.",
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
    let outcomes = response
        .results
        .into_iter()
        .map(edge_result)
        .collect::<Result<_, _>>()?;
    Ok(EdgeAnswer {
        checkpoint_sha256: response.checkpoint_sha256,
        outcomes,
    })
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

    fn response(results: serde_json::Value) -> EdgeJudgeResponse {
        serde_json::from_value(serde_json::json!({
            "request_id": "r1", "region": "hk", "model": "vulcan-decision-shell-v1-mini-preview",
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
            edge_outcomes("p1", "r1", 1, scored),
            Ok(EdgeAnswer {
                checkpoint_sha256: "a".repeat(64),
                outcomes: vec![EdgeOutcome::Evaluated {
                    label: JudgeLabel::Allow,
                    logits: [4.0, -2.0, -2.0],
                    input_tokens: 37
                }]
            })
        );
        let over = response(serde_json::json!([
            {"label": "block", "reason_code": "input_token_limit", "model_evaluated": false}
        ]));
        assert_eq!(
            edge_outcomes("p1", "r1", 1, over).map(|answer| answer.outcomes),
            Ok(vec![EdgeOutcome::InputTokenLimit])
        );
    }

    // Any well-formed checkpoint is accepted and reported; the client pins none.
    // 任何格式正确的检查点都被接受并报告；客户端不固定检查点。
    #[test]
    fn any_well_formed_checkpoint_is_accepted_and_reported() {
        let rotated = EdgeJudgeResponse {
            checkpoint_sha256: "0123456789abcdef".repeat(4),
            ..response(serde_json::json!([]))
        };
        assert_eq!(
            edge_outcomes("p1", "r1", 0, rotated).map(|answer| answer.checkpoint_sha256),
            Ok("0123456789abcdef".repeat(4))
        );
    }

    #[test]
    fn malformed_checkpoints_foreign_policies_requests_and_malformed_results_are_refused() {
        let scored = || {
            response(serde_json::json!([
                {"label": "allow", "logits": [4.0, -2.0, -2.0], "input_tokens": 37, "model_evaluated": true}
            ]))
        };
        for checkpoint in [
            "A".repeat(64),
            "a".repeat(63),
            "g".repeat(64),
            String::new(),
        ] {
            let malformed = EdgeJudgeResponse {
                checkpoint_sha256: checkpoint.clone(),
                ..scored()
            };
            assert!(
                edge_outcomes("p1", "r1", 1, malformed).is_err(),
                "{checkpoint}"
            );
        }
        assert!(edge_outcomes("p2", "r1", 1, scored()).is_err());
        assert!(edge_outcomes("p1", "r2", 1, scored()).is_err());
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
                edge_outcomes("p1", "r1", 1, response(results.clone())).is_err(),
                "{results}"
            );
        }
    }

    #[test]
    fn server_timing_reads_the_four_stages_and_ignores_the_rest() {
        assert_eq!(
            EdgeServerTiming::parse(
                "edge;dur=41, node;dur=33.5, queue;dur=1.2, compute;dur=30.0, cdn;dur=4"
            ),
            EdgeServerTiming {
                edge: Some(41.0),
                node: Some(33.5),
                queue: Some(1.2),
                compute: Some(30.0),
            }
        );
        assert_eq!(
            EdgeServerTiming::parse("edge;desc=\"x\";dur=7, node;dur=-1, queue;dur=abc, compute"),
            EdgeServerTiming {
                edge: Some(7.0),
                ..Default::default()
            }
        );
        assert_eq!(EdgeServerTiming::parse(""), EdgeServerTiming::default());
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
