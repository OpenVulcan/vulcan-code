//! TypeSafe System One decision wire profile.
//! TypeSafe System One 决策 Wire Profile。
//!
//! System One answers closed questions about a stated situation with one non-streaming JSON
//! document. It carries only `decision_evaluate`; conversation requests are refused rather than
//! approximated.
//! System One 以一份非流式 JSON 文档回答关于所陈述情境的封闭问题。它只承载 `decision_evaluate`；
//! 对话请求会被拒绝，而不是近似转换。

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;
use vulcan_ai_contract::{
    AiExecutionRequest, AiStreamEvent, DecisionAnswer, DecisionCriterion, DecisionQuestion,
    DecisionQuestionKind, DecisionRequest,
};

use crate::{
    EncodedWireRequest, ProtocolConversionError, ProtocolHttpMethod, ProtocolProfile,
    ProtocolStreamDecoder, ProtocolStreamFraming, ProtocolTransportProfile,
};

/// Stable profile identifier for System One decisions.
/// System One 决策的稳定 Profile 标识。
pub const TYPESAFE_SYSTEMONE_PROFILE_ID: &str = "typesafe-systemone.v1";

/// Encodes and decodes the System One decision contract.
/// 编解码 System One 决策契约。
pub struct TypeSafeSystemOneProfile;

impl ProtocolProfile for TypeSafeSystemOneProfile {
    /// Returns the stable System One profile identifier.
    /// 返回稳定的 System One Profile 标识。
    fn id(&self) -> &'static str {
        TYPESAFE_SYSTEMONE_PROFILE_ID
    }

    /// Returns the single-document JSON transport System One answers with.
    /// 返回 System One 应答所用的单文档 JSON 传输。
    fn transport_profile(&self) -> ProtocolTransportProfile {
        ProtocolTransportProfile {
            method: ProtocolHttpMethod::Post,
            request_content_type: "application/json",
            accepted_response_content_types: &["application/json"],
            framing: ProtocolStreamFraming::SingleJson,
            max_frame_bytes: 1024 * 1024,
            max_buffer_bytes: 1024 * 1024,
            max_tool_arguments_bytes: 0,
            max_error_body_bytes: 64 * 1024,
            response_headers_timeout_ms: 30_000,
            stream_idle_timeout_ms: 30_000,
        }
    }

    /// Refuses conversation requests; System One carries only decision actions.
    /// 拒绝对话请求；System One 只承载决策动作。
    fn encode(
        &self,
        _request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProtocolConversionError> {
        Err(conversation_refusal())
    }

    /// Returns a decoder that refuses every frame, since no conversation is ever encoded.
    /// 返回拒绝所有帧的解码器，因为从不编码对话。
    fn create_stream_decoder(
        &self,
        _execution_id: String,
        _request: &AiExecutionRequest,
    ) -> Box<dyn ProtocolStreamDecoder> {
        Box::new(RefusingDecoder)
    }
}

/// The error every conversation use of this profile returns.
/// 对该 Profile 的任何对话使用都返回的错误。
fn conversation_refusal() -> ProtocolConversionError {
    ProtocolConversionError::new(format!(
        "Protocol profile `{TYPESAFE_SYSTEMONE_PROFILE_ID}` carries only decision_evaluate requests."
    ))
}

/// Decoder for a profile that never streams conversations.
/// 从不流式传输对话的 Profile 所用的解码器。
struct RefusingDecoder;

impl ProtocolStreamDecoder for RefusingDecoder {
    /// Refuses the frame.
    /// 拒绝该帧。
    fn push_sse(
        &mut self,
        _event_name: Option<&str>,
        _data: &str,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        Err(conversation_refusal())
    }

    /// Refuses to finish a stream that was never valid.
    /// 拒绝结束一条从未有效的流。
    fn finish(&mut self) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        Err(conversation_refusal())
    }
}

/// The System One request body; questions and options keep their declared order.
/// System One 请求正文；问题与选项保持声明顺序。
pub struct SystemOneWireRequest<'a> {
    /// Exact model identifier.
    /// 精确模型标识。
    pub model: &'a str,
    /// The decision request being sent.
    /// 待发送的决策请求。
    pub request: &'a DecisionRequest,
}

impl Serialize for SystemOneWireRequest<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut body = serializer.serialize_map(Some(3))?;
        body.serialize_entry("model", self.model)?;
        body.serialize_entry("state", &self.request.state)?;
        body.serialize_entry("questions", &WireQuestions(&self.request.questions))?;
        body.end()
    }
}

/// Questions as an object keyed by question id, in declared order.
/// 按声明顺序、以问题标识为键的问题对象。
struct WireQuestions<'a>(&'a [DecisionQuestion]);

impl Serialize for WireQuestions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut questions = serializer.serialize_map(Some(self.0.len()))?;
        for question in self.0 {
            questions.serialize_entry(question.id.as_str(), &WireQuestion(&question.kind))?;
        }
        questions.end()
    }
}

/// One question in System One form.
/// System One 形式的一个问题。
struct WireQuestion<'a>(&'a DecisionQuestionKind);

impl Serialize for WireQuestion<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let DecisionQuestionKind::Choice {
            instructions,
            criteria,
        } = self.0;
        let mut question = serializer.serialize_map(Some(3))?;
        question.serialize_entry("type", "choice")?;
        question.serialize_entry("instructions", instructions)?;
        question.serialize_entry("criteria", &WireCriteria(criteria))?;
        question.end()
    }
}

/// Criteria as an object keyed by option, in declared order.
/// 按声明顺序、以选项为键的判据对象。
struct WireCriteria<'a>(&'a [DecisionCriterion]);

impl Serialize for WireCriteria<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut criteria = serializer.serialize_map(Some(self.0.len()))?;
        for criterion in self.0 {
            criteria.serialize_entry(criterion.option.as_str(), criterion.criterion.as_str())?;
        }
        criteria.end()
    }
}

/// Token counts System One reports.
/// System One 报告的 Token 数量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct SystemOneUsage {
    /// Input tokens.
    /// 输入 Token。
    #[serde(default)]
    pub input_tokens: Option<u64>,
    /// Output tokens.
    /// 输出 Token。
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

/// A decoded System One response.
/// 已解码的 System One 响应。
#[derive(Debug, Clone, PartialEq)]
pub struct SystemOneWireResponse {
    /// Model the provider reports having run.
    /// 供应商报告实际运行的模型。
    pub model: Option<String>,
    /// Provider response identifier, when reported.
    /// 供应商响应标识（如有报告）。
    pub response_id: Option<String>,
    /// Answers keyed by question id.
    /// 按问题标识索引的回答。
    pub answers: BTreeMap<String, DecisionAnswer>,
    /// Reported token counts.
    /// 已报告的 Token 数量。
    pub usage: SystemOneUsage,
}

/// Raw response shape; extra provider fields such as routing metadata are ignored.
/// 原始响应形状；忽略路由元数据等额外供应商字段。
#[derive(Deserialize)]
struct RawResponse {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    id: Option<String>,
    answers: BTreeMap<String, RawAnswer>,
    #[serde(default)]
    usage: SystemOneUsage,
}

/// Raw answer; only choice answers are defined by this profile.
/// 原始回答；本 Profile 只定义选择题回答。
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawAnswer {
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}

/// Decodes one System One response document.
/// 解码一份 System One 响应文档。
///
/// # Parameters
/// 参数
/// `body` is the complete UTF-8 response text.
/// `body` 是完整的 UTF-8 响应文本。
///
/// # Returns
/// 返回值
/// The decoded answers, or a conversion error naming the malformed part.
/// 已解码的回答，或指出格式错误部分的转换错误。
pub fn decode_systemone_response(
    body: &str,
) -> Result<SystemOneWireResponse, ProtocolConversionError> {
    let raw: RawResponse = serde_json::from_str(body).map_err(|error| {
        ProtocolConversionError::new(format!("System One response is not valid: {error}"))
    })?;
    Ok(SystemOneWireResponse {
        model: raw.model,
        response_id: raw.id,
        answers: raw
            .answers
            .into_iter()
            .map(|(id, answer)| {
                let RawAnswer::Choice {
                    choice,
                    probabilities,
                    confidence,
                } = answer;
                (
                    id,
                    DecisionAnswer::Choice {
                        choice,
                        probabilities,
                        confidence,
                    },
                )
            })
            .collect(),
        usage: raw.usage,
    })
}

/// Serializes a wire request to a value for secret-safe request capture.
/// 将 Wire 请求序列化为值，用于不含秘密的请求捕获。
///
/// # Returns
/// 返回值
/// The request body as JSON; object keys are re-sorted by `serde_json`.
/// JSON 形式的请求正文；对象键会被 `serde_json` 重新排序。
pub fn systemone_request_value(
    request: &SystemOneWireRequest<'_>,
) -> Result<Value, ProtocolConversionError> {
    serde_json::to_value(request).map_err(|error| {
        ProtocolConversionError::new(format!("System One request could not be encoded: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::INFERENCE_CONTRACT_VERSION;

    fn request() -> DecisionRequest {
        DecisionRequest {
            contract_version: INFERENCE_CONTRACT_VERSION,
            request_id: String::from("r"),
            idempotency_key: None,
            consumer_profile_id: String::from("permission_judge"),
            state: serde_json::json!({"call": {"tool": "shell_command"}}),
            questions: vec![DecisionQuestion {
                id: String::from("decision"),
                kind: DecisionQuestionKind::Choice {
                    instructions: String::from("Judge."),
                    criteria: ["review", "allow", "block"]
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

    #[test]
    fn request_keeps_declared_option_order() {
        let request = request();
        let text = serde_json::to_string(&SystemOneWireRequest {
            model: "jev-1.13.0",
            request: &request,
        })
        .expect("encode");
        let review = text.find("\"review\"").expect("review");
        let allow = text.find("\"allow\"").expect("allow");
        assert!(
            review < allow,
            "options must keep their declared order: {text}"
        );
        let value: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value["model"], "jev-1.13.0");
        assert_eq!(value["questions"]["decision"]["type"], "choice");
        assert_eq!(
            value["questions"]["decision"]["criteria"]["block"],
            "block."
        );
    }

    #[test]
    fn response_decodes_choice_answers_and_ignores_routing_metadata() {
        let decoded = decode_systemone_response(
            r#"{"model":"typesafe/jev-1.13-20260917","answers":{"decision":{"type":"choice","choice":"review","probabilities":{"allow":0.02,"review":0.73,"block":0.25},"confidence":0.59}},"usage":{"input_tokens":404,"output_tokens":38,"cost":1.6968e-05},"id":"gen-1","provider":"TypeSafe"}"#,
        )
        .expect("decode");
        assert_eq!(decoded.model.as_deref(), Some("typesafe/jev-1.13-20260917"));
        assert_eq!(decoded.response_id.as_deref(), Some("gen-1"));
        assert_eq!(decoded.usage.input_tokens, Some(404));
        let DecisionAnswer::Choice {
            choice,
            probabilities,
            ..
        } = &decoded.answers["decision"];
        assert_eq!(choice, "review");
        assert_eq!(probabilities.len(), 3);
        assert!(
            decode_systemone_response(r#"{"answers":{"d":{"type":"noul","noul":true}}}"#).is_err()
        );
    }

    #[test]
    fn conversation_requests_are_refused() {
        assert!(RefusingDecoder.push_sse(None, "{}").is_err());
        assert!(RefusingDecoder.finish().is_err());
    }
}
