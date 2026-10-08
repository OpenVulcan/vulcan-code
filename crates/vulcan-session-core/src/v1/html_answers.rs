//! Derive HTML answer descriptors from the canonical assistant event without copying its source.
//! 从权威助手事件派生 HTML 回答描述，不复制正文内容。

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{AssistantSemanticStatus, ConversationEvent, EventBody, EventVisibility};

pub mod templates;

/// Version accepted by the dedicated assistant Markdown fence.
/// 专用助手 Markdown 围栏所接受的版本。
pub const HTML_ANSWER_VERSION: u32 = 1;
/// Content-draft protocol, pinned to the built-in version-two renderer.
/// 内容草稿协议，固定使用内置版本二渲染器。
pub const HTML_ANSWER_DRAFT_VERSION: u32 = 2;
/// Maximum executable bytes in a single page; measured before browser parsing.
/// 单页可执行正文的最大字节数，在浏览器解析前计算。
pub const HTML_ANSWER_MAX_PAGE_BYTES: usize = 512 * 1024;
/// Maximum number of executable pages in one assistant event.
/// 一条助手事件中可执行页面的最大数量。
pub const HTML_ANSWER_MAX_PAGES: usize = 4;
/// Combined executable source budget in one assistant event.
/// 一条助手事件内全部可执行源码的合计预算。
pub const HTML_ANSWER_MAX_TOTAL_BYTES: usize = 1024 * 1024;
/// Maximum Unicode scalar count in a page title.
/// 页面标题的最大 Unicode 标量数量。
pub const HTML_ANSWER_MAX_TITLE_CHARS: usize = 120;
/// Lifetime of a ticket before its single exchange, in seconds.
/// 单次兑换前的票据有效时间，单位为秒。
pub const HTML_ANSWER_TICKET_SECONDS: u64 = 5 * 60;
/// Lifetime of an exchanged read-only page lease, in seconds.
/// 兑换后的只读页面租约有效时间，单位为秒。
pub const HTML_ANSWER_LEASE_SECONDS: u64 = 60 * 60;
/// Total memory budget for reconstructible descriptor caches, excluding original events.
/// 可重建描述缓存的总内存预算，不含原始事件。
pub const HTML_ANSWER_CACHE_BYTES: usize = 32 * 1024 * 1024;
/// Global pending ticket capacity.
/// 全局未兑换票据容量。
pub const HTML_ANSWER_MAX_TICKETS: usize = 1024;
/// Pending ticket capacity per authenticated client.
/// 每个已认证客户端的未兑换票据容量。
pub const HTML_ANSWER_MAX_CLIENT_TICKETS: usize = 32;
/// Global active lease capacity.
/// 全局活跃租约容量。
pub const HTML_ANSWER_MAX_LEASES: usize = 256;
/// Active lease capacity per authenticated client.
/// 每个已认证客户端的活跃租约容量。
pub const HTML_ANSWER_MAX_CLIENT_LEASES: usize = 8;
/// Sustained ticket issuance capacity per minute and client.
/// 每分钟每个客户端的持续票据签发容量。
pub const HTML_ANSWER_TICKETS_PER_MINUTE: u32 = 30;
/// Immediate ticket issuance burst per client.
/// 每个客户端允许的即时票据签发突发数量。
pub const HTML_ANSWER_TICKET_BURST: u32 = 5;
/// OS-generated secret entropy in bytes, before URL-safe encoding.
/// 操作系统生成的秘密熵字节数，编码为 URL 安全格式之前计算。
pub const HTML_ANSWER_SECRET_BYTES: usize = 32;
/// Maximum JSON body for ticket exchange or lease release, independent of generated page size.
/// 票据兑换或租约释放 JSON 正文上限，独立于生成页面大小。
pub const HTML_ANSWER_VIEW_REQUEST_MAX_BYTES: usize = 4096;
/// Parameterless notification a rendered page posts to close or restore its embedding viewer.
/// 渲染页面发送的无参数通知，用于关闭或还原其嵌入查看器。
pub const HTML_ANSWER_CLOSE_MESSAGE: &str = "vulcan-html-answer-close-v1";
/// Message type carrying only a rendered page's content height, so an inline viewer can size itself.
/// 仅携带渲染页面内容高度的消息类型，供内嵌查看器调整自身高度。
pub const HTML_ANSWER_HEIGHT_MESSAGE: &str = "vulcan-html-answer-height-v1";
/// Hexadecimal width of the complete SHA-256 result identity.
/// 完整 SHA-256 结果身份的十六进制宽度。
pub const HTML_ANSWER_ID_HEX_LENGTH: usize = 64;

/// Accept only the canonical lowercase full digest; return false for prefixes, paths or uppercase forms.
/// 仅接受规范小写完整摘要；前缀、路径或大写形式均返回假。
pub fn html_answer_identity_is_valid(value: &str) -> bool {
    value.len() == HTML_ANSWER_ID_HEX_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Exact source interval in UTF-8 bytes and JavaScript UTF-16 code units.
/// 按 UTF-8 字节和 JavaScript UTF-16 码元表示的精确源码区间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HtmlAnswerRange {
    /// Inclusive UTF-8 byte offset in the original assistant text.
    /// 原始助手正文中的 UTF-8 字节起点，包含该位置。
    pub byte_start: usize,
    /// Exclusive UTF-8 byte offset in the original assistant text.
    /// 原始助手正文中的 UTF-8 字节终点，不包含该位置。
    pub byte_end: usize,
    /// Inclusive UTF-16 code unit offset for the existing Markdown renderer.
    /// 供现有 Markdown 渲染器使用的 UTF-16 码元起点，包含该位置。
    pub utf16_start: usize,
    /// Exclusive UTF-16 code unit offset for the existing Markdown renderer.
    /// 供现有 Markdown 渲染器使用的 UTF-16 码元终点，不包含该位置。
    pub utf16_end: usize,
}

/// Why a recognized fence cannot be executed; clients localize these stable codes.
/// 已识别围栏无法执行的原因；客户端翻译这些稳定错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HtmlAnswerUnavailableReason {
    /// Metadata is not the exact version/title JSON object.
    /// 元信息不是精确的版本和标题 JSON 对象。
    InvalidMetadata,
    /// The declared protocol version is unsupported.
    /// 声明的协议版本不受支持。
    UnsupportedVersion,
    /// The title is empty, too long, or contains control characters.
    /// 标题为空、过长或包含控制字符。
    InvalidTitle,
    /// The CommonMark fence has no explicit closing marker.
    /// CommonMark 围栏没有显式闭合标记。
    UnclosedFence,
    /// The generation did not reach its successful final state.
    /// 生成未到达成功完成状态。
    Interrupted,
    /// The final assistant source has not been durably saved.
    /// 最终助手正文尚未可靠保存。
    NotPersisted,
    /// A page exceeds its own source budget.
    /// 页面超过其自身源码预算。
    PageTooLarge,
    /// The event already contains the maximum number of executable pages.
    /// 事件已包含允许的最大可执行页面数量。
    TooManyPages,
    /// The combined executable source exceeds the event budget.
    /// 可执行源码合计超过事件预算。
    TotalTooLarge,
    /// There is no HTML source between the two fence markers.
    /// 两个围栏标记之间没有 HTML 源码。
    EmptySource,
    /// The content draft has invalid components or exceeds renderer complexity limits.
    /// 内容草稿包含非法组件或超过渲染复杂度限制。
    InvalidDraft,
}

/// Execution authority is explicit and is never inferred by a client from code text.
/// 执行权威被显式表达，客户端绝不从代码文本推断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum HtmlAnswerAvailability {
    /// A durable completed event owns this precise immutable source interval.
    /// 一个可靠保存且已完成的事件拥有此精确不可变源码区间。
    Ready {
        /// Full domain-separated SHA-256 identity, including session, event, API and content.
        /// 包含会话、事件、API 和内容并带域隔离的完整 SHA-256 身份。
        artifact_id: String,
        /// Original source between the explicitly closed fence markers.
        /// 显式闭合围栏标记之间的原始源码。
        source: HtmlAnswerRange,
    },
    /// Preserve the original Markdown while explaining why execution is unavailable.
    /// 保留原始 Markdown，同时说明不可执行的原因。
    Unavailable {
        /// Stable machine-readable rejection reason.
        /// 稳定的机器可读拒绝原因。
        reason: HtmlAnswerUnavailableReason,
    },
}

/// Presentation metadata only; the original assistant text remains the sole content authority.
/// 仅包含展示元信息；原始助手正文始终是唯一内容权威。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HtmlAnswerDescriptor {
    /// Zero-based ordinal among recognized top-level dedicated fences, including rejected ones.
    /// 在已识别顶层专用围栏中的零基序号，包含被拒绝的围栏。
    pub ordinal: usize,
    /// Validated plain-text title; absent when metadata validation failed.
    /// 校验后的纯文本标题；元信息校验失败时不存在。
    pub title: Option<String>,
    /// Entire original fenced code node, including its markers.
    /// 包含标记的完整原始围栏代码节点。
    pub fence: HtmlAnswerRange,
    /// Core-derived execution status for this source revision.
    /// Core 派生的此源码修订执行状态。
    pub availability: HtmlAnswerAvailability,
}

/// Derived annotations bound to one exact durable event revision, separate from canonical facts.
/// 绑定到精确可靠事件修订的派生标注，与权威事实分离。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HtmlAnswerEventProjection {
    /// Owning assistant event sequence within the enclosing session and generation.
    /// 外层会话及代次中的所属助手事件序号。
    pub seq: super::Seq,
    /// Exact source revision used to derive all descriptors.
    /// 派生全部描述时使用的精确源码修订。
    pub event_revision: u64,
    /// Descriptors, possibly empty after examining an inert example; no source copies or browser state.
    /// 描述集合，检查不可执行示例后可以为空；不包含源码副本或浏览器状态。
    pub answers: Vec<HtmlAnswerDescriptor>,
}

impl HtmlAnswerEventProjection {
    /// Project `event` only when captured from confirmed storage or a successfully flushed candidate.
    /// 仅投影从确认存储或成功落盘候选中捕获的 `event`。
    /// `durable` is physical publication evidence; event and document revisions are different counters.
    /// `durable` 为物理发布证据；事件修订和文档修订是不同的计数器。
    /// `session_id` is the enclosing canonical owner, never a model-authored value.
    /// `session_id` 为外层权威归属，绝不能取自模型生成内容。
    pub fn from_event(session_id: &str, event: &ConversationEvent, durable: bool) -> Option<Self> {
        if !durable {
            return None;
        }
        let EventBody::AssistantText(assistant) = &event.event else {
            return None;
        };
        if event.visibility != EventVisibility::Provider
            || assistant.status == AssistantSemanticStatus::Streaming
            || !assistant.text.contains("vulcan-html")
        {
            return None;
        }
        // An empty result still acknowledges examined examples, avoiding repeated recovery scans.
        // 空结果仍确认已检查的示例，避免重连时反复扫描。
        let answers = project_html_answers(session_id, event, true);
        Some(Self {
            seq: event.seq,
            event_revision: event.event_revision,
            answers,
        })
    }
}

/// Strict model-authored fence metadata; serde rejects duplicate and unknown fields.
/// 严格的模型生成围栏元信息；serde 拒绝重复字段和未知字段。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FenceMetadata {
    /// Explicit protocol version.
    /// 显式协议版本。
    version: u32,
    /// Model-provided plain-text title.
    /// 模型提供的纯文本标题。
    title: String,
    /// Required only for draft version two; explicit null is never a valid template.
    /// 仅草稿版本二要求此字段；显式 null 不是合法模板。
    #[serde(default, deserialize_with = "deserialize_template")]
    template: Option<templates::HtmlAnswerTemplate>,
}

/// Deserialize a present template field; return its typed value and reject null or unknown names.
/// 反序列化已出现的模板字段；返回强类型值，拒绝 null 和未知名称。
fn deserialize_template<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<templates::HtmlAnswerTemplate>, D::Error> {
    templates::HtmlAnswerTemplate::deserialize(deserializer).map(Some)
}

/// Select the immutable renderer declared by a validated original fence; never inspect body heuristics.
/// 从已校验的原始围栏选择不可变渲染器；绝不启发式探测正文。
/// Returns None for raw version one, or the exact version-two template; errors reject the declaration.
/// 版本一返回空，版本二返回精确模板；错误直接拒绝声明。
pub fn html_answer_template(
    fence: &str,
) -> Result<Option<templates::HtmlAnswerTemplate>, HtmlAnswerUnavailableReason> {
    let line = fence
        .lines()
        .next()
        .ok_or(HtmlAnswerUnavailableReason::InvalidMetadata)?;
    let info = line.trim_start_matches(' ').trim_start_matches('`');
    let json = dedicated_metadata(info).ok_or(HtmlAnswerUnavailableReason::InvalidMetadata)?;
    validate_metadata(json).map(|metadata| metadata.template)
}

/// Monotonic conversion cursor prevents repeated scans of a long source prefix.
/// 单调转换游标避免反复扫描长源码前缀。
#[derive(Default)]
struct SourceCursor {
    /// Last converted byte boundary.
    /// 最近转换的字节边界。
    byte: usize,
    /// Corresponding UTF-16 boundary.
    /// 对应的 UTF-16 边界。
    utf16: usize,
}

impl SourceCursor {
    /// Convert an ordered boundary in `text`, returning its UTF-16 code-unit position.
    /// 转换 `text` 中有序的边界，返回其 UTF-16 码元位置。
    fn advance(&mut self, text: &str, byte: usize) -> usize {
        // HTML syntax is usually ASCII: its byte and UTF-16 lengths are identical.
        // HTML 语法通常为 ASCII：其字节长度与 UTF-16 长度相同。
        // Scan non-ASCII spans with the exact encoder so astral characters retain two code units.
        // 非 ASCII 区间使用精确编码器，确保补充平面字符仍占两个码元。
        let span = &text[self.byte..byte];
        self.utf16 += if span.is_ascii() {
            span.len()
        } else {
            span.encode_utf16().count()
        };
        self.byte = byte;
        self.utf16
    }
}

/// Inspect a terminal provider-visible assistant event in its owning `session_id`.
/// 检查属于 `session_id` 的终态且对供应商可见的助手事件。
/// `durable` must come from successful storage, not the model; returns descriptors without HTML copies.
/// `durable` 必须来自成功存储，不能来自模型；返回不含 HTML 副本的描述。
/// Streaming events return no executable descriptors and are parsed once upon terminal projection.
/// 流式事件不返回可执行描述，在终态投影时统一解析。
pub fn project_html_answers(
    session_id: &str,
    event: &ConversationEvent,
    durable: bool,
) -> Vec<HtmlAnswerDescriptor> {
    let EventBody::AssistantText(assistant) = &event.event else {
        return Vec::new();
    };
    if event.visibility != EventVisibility::Provider
        || assistant.status == AssistantSemanticStatus::Streaming
        || !assistant.text.contains("vulcan-html")
    {
        return Vec::new();
    }
    // Keep GFM structural containers enabled so fenced examples in footnotes stay non-executable.
    // 启用 GFM 结构容器，确保脚注中的围栏示例不可执行。
    let parser = Parser::new_ext(&assistant.text, Options::all()).into_offset_iter();
    let mut depth = 0usize;
    let mut descriptors = Vec::new();
    let mut accepted_count = 0usize;
    let mut accepted_bytes = 0usize;
    let mut cursor = SourceCursor::default();
    for (markdown, range) in parser {
        match markdown {
            Event::Start(tag) => {
                if depth == 0
                    && let Tag::CodeBlock(CodeBlockKind::Fenced(info)) = &tag
                    && let Some(metadata) = dedicated_metadata(info)
                    && assistant.text.as_bytes()[range.start] == b'`'
                {
                    let ordinal = descriptors.len();
                    let checked_metadata = validate_metadata(metadata);
                    let title = checked_metadata
                        .as_ref()
                        .ok()
                        .map(|meta| meta.title.clone());
                    let source_range = closed_source_range(&assistant.text, &range);
                    let fence_start = cursor.advance(&assistant.text, range.start);
                    let source = source_range.as_ref().map(|source| HtmlAnswerRange {
                        byte_start: source.start,
                        byte_end: source.end,
                        utf16_start: cursor.advance(&assistant.text, source.start),
                        utf16_end: cursor.advance(&assistant.text, source.end),
                    });
                    let fence = HtmlAnswerRange {
                        byte_start: range.start,
                        byte_end: range.end,
                        utf16_start: fence_start,
                        utf16_end: cursor.advance(&assistant.text, range.end),
                    };
                    let checked = checked_metadata
                        .as_ref()
                        .map_err(|reason| *reason)
                        .and_then(|metadata| {
                            let source =
                                source.ok_or(HtmlAnswerUnavailableReason::UnclosedFence)?;
                            let bytes = source.byte_end - source.byte_start;
                            if assistant.status != AssistantSemanticStatus::Completed {
                                return Err(HtmlAnswerUnavailableReason::Interrupted);
                            }
                            if !durable {
                                return Err(HtmlAnswerUnavailableReason::NotPersisted);
                            }
                            if assistant.text[source.byte_start..source.byte_end]
                                .trim()
                                .is_empty()
                            {
                                return Err(HtmlAnswerUnavailableReason::EmptySource);
                            }
                            if bytes > HTML_ANSWER_MAX_PAGE_BYTES {
                                return Err(HtmlAnswerUnavailableReason::PageTooLarge);
                            }
                            if accepted_count >= HTML_ANSWER_MAX_PAGES {
                                return Err(HtmlAnswerUnavailableReason::TooManyPages);
                            }
                            if bytes > HTML_ANSWER_MAX_TOTAL_BYTES - accepted_bytes {
                                return Err(HtmlAnswerUnavailableReason::TotalTooLarge);
                            }
                            if let Some(template) = metadata.template {
                                templates::render_draft(
                                    template,
                                    &metadata.title,
                                    &assistant.text[source.byte_start..source.byte_end],
                                )?;
                            }
                            accepted_count += 1;
                            accepted_bytes += bytes;
                            Ok(source)
                        });
                    let availability = match checked {
                        Ok(source) => HtmlAnswerAvailability::Ready {
                            artifact_id: artifact_identity(
                                session_id,
                                event,
                                assistant.api_call_id.value(),
                                ordinal,
                                &assistant.text[source.byte_start..source.byte_end],
                                checked_metadata.as_ref().expect("validated metadata"),
                            ),
                            source,
                        },
                        Err(reason) => HtmlAnswerAvailability::Unavailable { reason },
                    };
                    descriptors.push(HtmlAnswerDescriptor {
                        ordinal,
                        title,
                        fence,
                        availability,
                    });
                }
                depth += 1;
            }
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    descriptors
}

/// Return only the JSON suffix of the exact dedicated language token in `info`.
/// 仅从 `info` 中返回精确专用语言标记之后的 JSON 后缀。
fn dedicated_metadata(info: &str) -> Option<&str> {
    let suffix = info.strip_prefix("vulcan-html")?;
    if suffix.is_empty() || suffix.starts_with(char::is_whitespace) {
        Some(suffix.trim())
    } else {
        None
    }
}

/// Validate one metadata JSON object, returning a precise stable failure category.
/// 校验一个元信息 JSON 对象，返回精确稳定的失败分类。
fn validate_metadata(json: &str) -> Result<FenceMetadata, HtmlAnswerUnavailableReason> {
    let metadata: FenceMetadata =
        serde_json::from_str(json).map_err(|_| HtmlAnswerUnavailableReason::InvalidMetadata)?;
    if metadata.version != HTML_ANSWER_VERSION && metadata.version != HTML_ANSWER_DRAFT_VERSION {
        return Err(HtmlAnswerUnavailableReason::UnsupportedVersion);
    }
    if (metadata.version == HTML_ANSWER_DRAFT_VERSION) != metadata.template.is_some() {
        return Err(HtmlAnswerUnavailableReason::InvalidMetadata);
    }
    if metadata.title.trim().is_empty()
        || metadata.title.chars().count() > HTML_ANSWER_MAX_TITLE_CHARS
        || metadata.title.chars().any(char::is_control)
    {
        return Err(HtmlAnswerUnavailableReason::InvalidTitle);
    }
    Ok(metadata)
}

/// Validate the physical closing line of a CommonMark node; return its original HTML interval.
/// 校验 CommonMark 节点的物理闭合行；返回其原始 HTML 区间。
/// CommonMark's implicit EOF closure must never authorize execution.
/// CommonMark 在文件结尾处的隐式闭合绝不能授权执行。
fn closed_source_range(text: &str, fence: &Range<usize>) -> Option<Range<usize>> {
    let node = &text[fence.clone()];
    let opener_ticks = node.bytes().take_while(|byte| *byte == b'`').count();
    let opening_end = node.find('\n')? + 1;
    let without_eol = node.trim_end_matches(['\r', '\n']);
    let closing_start = without_eol.rfind('\n')? + 1;
    if closing_start < opening_end {
        return None;
    }
    let closing_line = &without_eol[closing_start..];
    let indent = closing_line
        .bytes()
        .take_while(|byte| *byte == b' ')
        .count();
    if indent > 3 {
        return None;
    }
    let closing = &closing_line[indent..];
    let ticks = closing.bytes().take_while(|byte| *byte == b'`').count();
    if ticks < opener_ticks
        || !closing[ticks..]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'))
    {
        return None;
    }
    Some(fence.start + opening_end..fence.start + closing_start)
}

/// Hash the owning event, ordinal, version, title and source using length-prefixed fields.
/// 使用长度前缀字段散列所属事件、序号、版本、标题和源码。
/// Content binding prevents truncated/reused event positions from reviving an old result identity.
/// 内容绑定防止截断后复用事件位置时复活旧结果身份。
fn artifact_identity(
    session_id: &str,
    event: &ConversationEvent,
    api_call_id: u64,
    ordinal: usize,
    source: &str,
    metadata: &FenceMetadata,
) -> String {
    let mut hash = Sha256::new();
    for field in [
        b"vulcan.html-answer.v1".as_slice(),
        session_id.as_bytes(),
        &event.seq.value().to_le_bytes(),
        &api_call_id.to_le_bytes(),
        &(ordinal as u64).to_le_bytes(),
        &metadata.version.to_le_bytes(),
        metadata.title.as_bytes(),
        Sha256::digest(source.as_bytes()).as_slice(),
    ] {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field);
    }
    // Preserve every version-one digest while binding the draft layout to its result identity.
    // 保持所有版本一摘要，同时将草稿布局绑定到结果身份。
    if let Some(template) = metadata.template {
        let name = template.name();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests;
