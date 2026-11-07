//! Turns a user's draft into the exact request bytes they preview and later send.
//! 把用户草稿转换为其预览并在之后发送的精确请求字节。

use crate::limits::{
    BODY_MAX_CHARS, ERROR_MESSAGE_MAX_CHARS, ERROR_REQUEST_ID_MAX_CHARS, LOG_EXCERPT_MAX_BYTES,
    LOG_EXCERPT_MAX_LINES, REDACTION_HIT_MAX_COUNT, REDACTION_HITS_MAX_CATEGORIES,
    REDACTION_RULES_VERSION_MAX_CHARS, REQUEST_MAX_BYTES, SHORT_TEXT_MAX_CHARS, TITLE_MAX_CHARS,
};
use crate::model::{
    FeedbackAttachments, FeedbackClientInfo, FeedbackCreateRequest, FeedbackErrorAttachment,
    FeedbackKind, FeedbackRedactionSummary, FeedbackSource,
};
use crate::redact::{RedactionHits, Redactor};
use crate::{FeedbackError, FeedbackErrorCode};
use sha2::{Digest, Sha256};

/// Unfiltered draft as the user and the collectors left it.
/// 用户与采集器留下的未过滤草稿。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackDraft {
    /// Idempotency key kept for the life of the draft.
    /// 在草稿生命周期内保持不变的幂等键。
    pub client_request_id: String,
    /// Suggestion or problem.
    /// 建议或问题。
    pub kind: FeedbackKind,
    /// Entry that produced the draft.
    /// 草稿来源入口。
    pub source: FeedbackSource,
    /// Title as typed.
    /// 用户输入的标题。
    pub title: String,
    /// Description as typed.
    /// 用户输入的描述。
    pub body: String,
    /// Whether maintainers may reply.
    /// 是否允许维护者回复。
    pub allow_contact: bool,
    /// Environment, when ticked.
    /// 环境信息（勾选时）。
    pub client: Option<FeedbackClientInfo>,
    /// Failure facts, when ticked and available.
    /// 失败事实（勾选且存在时）。
    pub error: Option<FeedbackErrorAttachment>,
    /// Raw selected log lines, when ticked.
    /// 选出的原始日志行（勾选时）。
    pub log_lines: Option<Vec<String>>,
}

/// Filtered request plus what the preview shows about it.
/// 过滤后的请求，以及预览展示的相关信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFeedback {
    /// Request exactly as it will be sent.
    /// 将原样发送的请求。
    pub request: FeedbackCreateRequest,
    /// Encoded request body.
    /// 编码后的请求体。
    pub encoded: Vec<u8>,
    /// Hex SHA-256 of `encoded`; submission refuses a different value.
    /// `encoded` 的十六进制 SHA-256；提交时不一致即拒绝。
    pub digest: String,
}

/// Filters, bounds and encodes one draft.
/// 过滤、限长并编码一个草稿。
///
/// # Parameters
/// 参数
/// `draft` is the unfiltered draft and `redactor` the compiled filter for this machine.
/// `draft` 是未过滤草稿，`redactor` 是本机已编译的过滤器。
///
/// # Returns
/// 返回值
/// The prepared request, or a coded error the interface maps to a readable explanation.
/// 准备好的请求；否则返回界面可映射为可读说明的错误码。
pub fn prepare_feedback(
    draft: FeedbackDraft,
    redactor: &Redactor,
) -> Result<PreparedFeedback, FeedbackError> {
    let client_request_id = draft.client_request_id.trim().to_owned();
    if !is_hyphenated_uuid(&client_request_id) {
        return Err(FeedbackError::new(
            FeedbackErrorCode::InvalidField,
            "client_request_id",
        ));
    }
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(FeedbackError::new(
            FeedbackErrorCode::TitleRequired,
            "title",
        ));
    }
    if title.chars().count() > TITLE_MAX_CHARS {
        return Err(FeedbackError::new(FeedbackErrorCode::TitleTooLong, "title"));
    }
    let body = draft.body.trim();
    if body.is_empty() {
        return Err(FeedbackError::new(FeedbackErrorCode::BodyRequired, "body"));
    }
    if body.chars().count() > BODY_MAX_CHARS {
        return Err(FeedbackError::new(FeedbackErrorCode::BodyTooLong, "body"));
    }
    if let Some(client) = &draft.client {
        let fields = [
            &client.version,
            &client.channel,
            &client.os,
            &client.arch,
            &client.locale,
        ];
        if fields
            .iter()
            .any(|field| field.is_empty() || field.chars().count() > SHORT_TEXT_MAX_CHARS)
        {
            return Err(FeedbackError::new(
                FeedbackErrorCode::InvalidField,
                "client",
            ));
        }
    }

    let mut hits = RedactionHits::new();
    let title = truncate_chars(&redactor.redact(title, &mut hits)?, TITLE_MAX_CHARS);
    let body = truncate_chars(&redactor.redact(body, &mut hits)?, BODY_MAX_CHARS);
    let error = match draft.error {
        Some(error) => Some(filter_error(error, redactor, &mut hits)?),
        None => None,
    };
    let log_excerpt = match draft.log_lines {
        Some(lines) => filter_log_lines(&lines, redactor, &mut hits)?,
        None => None,
    };

    let request = FeedbackCreateRequest {
        client_request_id,
        kind: draft.kind,
        title,
        body,
        allow_contact: draft.allow_contact,
        client: draft.client,
        attachments: FeedbackAttachments {
            source: draft.source,
            error,
            log_excerpt,
        },
        redaction: FeedbackRedactionSummary {
            rules_version: truncate_chars(
                redactor.rules_version(),
                REDACTION_RULES_VERSION_MAX_CHARS,
            ),
            hits: bounded_hits(hits),
        },
    };
    let encoded = serde_json::to_vec(&request)
        .map_err(|error| FeedbackError::new(FeedbackErrorCode::FilterFailed, error.to_string()))?;
    if encoded.len() > REQUEST_MAX_BYTES {
        return Err(FeedbackError::new(
            FeedbackErrorCode::RequestTooLarge,
            encoded.len().to_string(),
        ));
    }
    let digest = hex::encode(Sha256::digest(&encoded));
    Ok(PreparedFeedback {
        request,
        encoded,
        digest,
    })
}

/// Tells whether `value` is a hyphenated UUID (8-4-4-4-12 hexadecimal digits), as the contract requires.
/// 判断 `value` 是否为契约要求的带连字符 UUID（8-4-4-4-12 位十六进制）。
fn is_hyphenated_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, length)| {
            group.len() == length && group.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

/// Caps each count and folds the rarest categories into `other` beyond the category limit.
/// 限制每个次数，并在超出类别上限时把最少见的类别并入 `other`。
fn bounded_hits(hits: RedactionHits) -> RedactionHits {
    const OTHER: &str = "other";
    if hits.len() <= REDACTION_HITS_MAX_CATEGORIES {
        return hits
            .into_iter()
            .map(|(name, count)| (name, count.min(REDACTION_HIT_MAX_COUNT)))
            .collect();
    }
    let mut ranked: Vec<(String, u32)> = hits.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let mut bounded = RedactionHits::new();
    let mut other: u32 = 0;
    for (index, (name, count)) in ranked.into_iter().enumerate() {
        if index < REDACTION_HITS_MAX_CATEGORIES - 1 && name != OTHER {
            bounded.insert(name, count.min(REDACTION_HIT_MAX_COUNT));
        } else {
            other = other.saturating_add(count);
        }
    }
    bounded.insert(OTHER.to_owned(), other.min(REDACTION_HIT_MAX_COUNT));
    bounded
}

/// Filters every text field of the failure facts and bounds its length.
/// 过滤失败事实的每个文本字段并限制长度。
fn filter_error(
    error: FeedbackErrorAttachment,
    redactor: &Redactor,
    hits: &mut RedactionHits,
) -> Result<FeedbackErrorAttachment, FeedbackError> {
    let mut field =
        |value: Option<String>, limit: usize| -> Result<Option<String>, FeedbackError> {
            value
                .filter(|text| !text.trim().is_empty())
                .map(|text| Ok(truncate_chars(&redactor.redact(&text, hits)?, limit)))
                .transpose()
        };
    let category = field(Some(error.category), SHORT_TEXT_MAX_CHARS)?
        .ok_or_else(|| FeedbackError::new(FeedbackErrorCode::InvalidField, "error.category"))?;
    let subcategory = field(error.subcategory, SHORT_TEXT_MAX_CHARS)?;
    let stage = field(error.stage, SHORT_TEXT_MAX_CHARS)?;
    let provider_id = field(error.provider_id, SHORT_TEXT_MAX_CHARS)?;
    let model_id = field(error.model_id, SHORT_TEXT_MAX_CHARS)?;
    let upstream_code = field(error.upstream_code, SHORT_TEXT_MAX_CHARS)?;
    let request_id = field(error.request_id, ERROR_REQUEST_ID_MAX_CHARS)?;
    let message = error
        .message
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            Ok::<_, FeedbackError>(truncate_chars(
                &redactor.redact(&text, hits)?,
                ERROR_MESSAGE_MAX_CHARS,
            ))
        })
        .transpose()?;
    Ok(FeedbackErrorAttachment {
        category,
        subcategory,
        stage,
        http_status: error
            .http_status
            .filter(|status| (100..=599).contains(status)),
        provider_id,
        model_id,
        upstream_code,
        request_id,
        retryable: error.retryable,
        message,
    })
}

/// Filters each line, then keeps the newest lines that fit the excerpt limits.
/// 逐行过滤，然后保留符合摘录上限的最新行。
fn filter_log_lines(
    lines: &[String],
    redactor: &Redactor,
    hits: &mut RedactionHits,
) -> Result<Option<String>, FeedbackError> {
    let start = lines.len().saturating_sub(LOG_EXCERPT_MAX_LINES);
    let mut kept = Vec::new();
    for line in &lines[start..] {
        kept.push(redactor.redact(line, hits)?);
    }
    let mut dropped = start;
    let mut total: usize = kept.iter().map(|line| line.len() + 1).sum();
    let mut first = 0;
    while total > LOG_EXCERPT_MAX_BYTES && first < kept.len() {
        total -= kept[first].len() + 1;
        first += 1;
        dropped += 1;
    }
    let kept = &kept[first..];
    if kept.is_empty() {
        return Ok(None);
    }
    let mut excerpt = String::new();
    if dropped > 0 {
        excerpt.push_str(&format!("[truncated {dropped} earlier lines]\n"));
    }
    excerpt.push_str(&kept.join("\n"));
    Ok(Some(excerpt))
}

/// Cuts a text to at most `limit` characters without splitting a character.
/// 把文本截断为最多 `limit` 个字符，不拆分字符。
fn truncate_chars(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redact::{KnownSecret, RedactorInputs};

    fn redactor() -> Redactor {
        Redactor::new(RedactorInputs {
            secrets: vec![KnownSecret {
                label: String::from("API_KEY"),
                value: String::from("s3cr3t-value"),
            }],
            locale: String::from("en-US"),
            ..RedactorInputs::default()
        })
        .unwrap()
    }

    fn draft() -> FeedbackDraft {
        FeedbackDraft {
            client_request_id: String::from("8c1d0f3a-0000-4000-8000-000000000001"),
            kind: FeedbackKind::Bug,
            source: FeedbackSource::ErrorCard,
            title: String::from("  Retry loop  "),
            body: String::from("key s3cr3t-value leaked"),
            allow_contact: true,
            client: None,
            error: Some(FeedbackErrorAttachment {
                category: String::from("rate_limited"),
                http_status: Some(429),
                message: Some(String::from("bad key s3cr3t-value")),
                ..FeedbackErrorAttachment::default()
            }),
            log_lines: Some(vec![String::from(
                "{\"level\":\"ERROR\",\"k\":\"s3cr3t-value\"}",
            )]),
        }
    }

    #[test]
    fn preparation_filters_every_text_and_digests_the_exact_bytes() {
        let prepared = prepare_feedback(draft(), &redactor()).unwrap();
        let text = String::from_utf8(prepared.encoded.clone()).unwrap();
        assert!(!text.contains("s3cr3t-value"));
        assert_eq!(prepared.request.title, "Retry loop");
        assert_eq!(prepared.request.redaction.hits.get("secret"), Some(&3));
        assert_eq!(
            prepared.digest,
            hex::encode(Sha256::digest(&prepared.encoded))
        );
        let again = prepare_feedback(draft(), &redactor()).unwrap();
        assert_eq!(prepared.digest, again.digest);
        let decoded: FeedbackCreateRequest = serde_json::from_slice(&prepared.encoded).unwrap();
        assert_eq!(decoded, prepared.request);
    }

    #[test]
    fn empty_and_oversized_user_text_is_refused_with_a_code() {
        let mut empty = draft();
        empty.title = String::from("   ");
        assert_eq!(
            prepare_feedback(empty, &redactor()).unwrap_err().code(),
            FeedbackErrorCode::TitleRequired
        );
        let mut long = draft();
        long.body = "x".repeat(BODY_MAX_CHARS + 1);
        assert_eq!(
            prepare_feedback(long, &redactor()).unwrap_err().code(),
            FeedbackErrorCode::BodyTooLong
        );
    }

    #[test]
    fn long_logs_keep_the_newest_lines_within_the_byte_limit() {
        let mut big = draft();
        let line = format!("{{\"level\":\"ERROR\",\"m\":\"{}\"}}", "y".repeat(1000));
        big.log_lines = Some(vec![line; LOG_EXCERPT_MAX_LINES]);
        let prepared = prepare_feedback(big, &redactor()).unwrap();
        let excerpt = prepared.request.attachments.log_excerpt.unwrap();
        assert!(excerpt.len() <= LOG_EXCERPT_MAX_BYTES + 64);
        assert!(excerpt.starts_with("[truncated "));
    }

    #[test]
    fn unticked_attachments_are_absent_from_the_wire() {
        let mut bare = draft();
        bare.error = None;
        bare.log_lines = None;
        let prepared = prepare_feedback(bare, &redactor()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&prepared.encoded).unwrap();
        assert!(value.get("client").is_none());
        assert_eq!(
            value["attachments"],
            serde_json::json!({ "source": "error_card" })
        );
    }

    #[test]
    fn redaction_hits_respect_the_contract_bounds() {
        let many: RedactionHits = (0..40u32)
            .map(|index| (format!("rule_{index:02}"), index + 1))
            .collect();
        let bounded = bounded_hits(many);
        assert_eq!(bounded.len(), REDACTION_HITS_MAX_CATEGORIES);
        assert_eq!(
            bounded.values().map(|count| u64::from(*count)).sum::<u64>(),
            820
        );
        assert!(bounded.contains_key("other"));
        let large: RedactionHits = [(String::from("path"), u32::MAX)].into_iter().collect();
        assert_eq!(bounded_hits(large)["path"], REDACTION_HIT_MAX_COUNT);
    }

    #[test]
    fn request_ids_must_be_hyphenated_uuids() {
        assert!(is_hyphenated_uuid("8c1d0f3a-0000-4000-8000-000000000001"));
        assert!(!is_hyphenated_uuid("8c1d0f3a00004000800000000000001"));
        assert!(!is_hyphenated_uuid("8c1d0f3a-0000-4000-8000-00000000000g"));
    }
}
