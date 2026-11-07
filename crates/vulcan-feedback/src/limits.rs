//! Size limits of one feedback submission.
//! 单条反馈提交的大小上限。
//!
//! These mirror the OpenVulcan user center contract (`contracts/openapi-vulcancode.yaml` in
//! openvulcan-web), which stays authoritative: the server still rejects anything larger. Every other
//! place in Vulcan Code derives its limits from this module instead of repeating a number.
//! 这些值镜像 OpenVulcan 用户中心的契约（openvulcan-web 中的 `contracts/openapi-vulcancode.yaml`），
//! 服务端仍是权威并会拒绝更大的内容。Vulcan Code 其他位置一律从本模块派生上限，不重复数字。

/// Maximum characters of the title after trimming.
/// 去除首尾空白后标题的最大字符数。
pub const TITLE_MAX_CHARS: usize = 120;

/// Maximum characters of the description.
/// 描述的最大字符数。
pub const BODY_MAX_CHARS: usize = 8_000;

/// Maximum characters of the filtered provider error message.
/// 过滤后供应商错误消息的最大字符数。
pub const ERROR_MESSAGE_MAX_CHARS: usize = 2_000;

/// Maximum characters of one short contract field: client environment fields (version, platform,
/// locale) and error codes and identifiers (`ShortText` in the user-center contract).
/// 单个短契约字段的最大字符数：客户端环境字段（版本、平台、语言）与错误码、标识（用户中心契约中的
/// `ShortText`）。
pub const SHORT_TEXT_MAX_CHARS: usize = 64;

/// Maximum characters of the upstream request id in an error attachment.
/// 错误附件中上游请求编号的最大字符数。
pub const ERROR_REQUEST_ID_MAX_CHARS: usize = 128;

/// Maximum characters of the redaction rules version.
/// 过滤规则版本的最大字符数。
pub const REDACTION_RULES_VERSION_MAX_CHARS: usize = 64;

/// Maximum redaction categories reported; rarer categories fold into `other`.
/// 报告的最多过滤类别数；更少见的类别并入 `other`。
pub const REDACTION_HITS_MAX_CATEGORIES: usize = 32;

/// Largest count reported for one redaction category.
/// 单个过滤类别报告的最大次数。
pub const REDACTION_HIT_MAX_COUNT: u32 = 1_000_000;

/// Maximum number of log lines in one excerpt.
/// 单个日志摘录的最大行数。
pub const LOG_EXCERPT_MAX_LINES: usize = 200;

/// Maximum UTF-8 bytes of one log excerpt.
/// 单个日志摘录的最大 UTF-8 字节数。
pub const LOG_EXCERPT_MAX_BYTES: usize = 32 * 1024;

/// Maximum UTF-8 bytes of the complete encoded request body.
/// 完整编码请求体的最大 UTF-8 字节数。
pub const REQUEST_MAX_BYTES: usize = 64 * 1024;

/// Maximum bytes read from the end of each service log segment while choosing excerpt lines.
/// 选取摘录行时从每个服务日志分段末尾读取的最大字节数。
pub const LOG_SCAN_MAX_BYTES_PER_SEGMENT: u64 = 1024 * 1024;

/// Number of most recent service log segments scanned (the active one plus newer archives).
/// 扫描的最近服务日志分段数量（活动分段加较新的归档）。
pub const LOG_SCAN_SEGMENTS: usize = 2;

/// Prepared drafts kept in memory awaiting the user's send; the oldest is dropped first.
/// 内存中等待用户发送的已准备草稿数量；最旧的先被丢弃。
pub const PREPARED_DRAFTS_KEPT: usize = 16;
