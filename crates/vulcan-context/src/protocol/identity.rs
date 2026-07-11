//! Canonical conversation-record identity validation.
//! 规范会话记录身份校验。

use super::types::SessionConversationMessage;
use super::types::SessionConversationRecord;
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

/// Internal failure produced while registering one canonical protocol-message identifier.
/// 注册一个规范协议消息标识时产生的内部失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalMessageIdentifierError {
    /// The identifier is empty after removing insignificant surrounding whitespace.
    /// 标识移除无意义首尾空白后为空。
    Missing,
    /// The identifier contains surrounding whitespace and is not canonical.
    /// 标识包含首尾空白，因而不是规范形式。
    NonCanonical {
        /// Exact non-canonical identifier supplied by the caller.
        /// 调用方提供的原始非规范标识。
        identifier: String,
    },
    /// The identifier has already been registered in the same conversation window.
    /// 标识已经在同一会话窗口中注册。
    Duplicate {
        /// Exact identifier claimed by more than one canonical message.
        /// 被多条规范消息占用的精确标识。
        identifier: String,
    },
}

/// Registers one canonical message identifier without rewriting its identity.
/// 注册一个规范消息标识，且不改写其身份。
///
/// # Parameters
/// 参数
/// `identifiers` is the conversation-wide set of identifiers already accepted.
/// `identifiers` 是会话范围内已经接受的标识集合。
/// `identifier` is the exact local protocol-message identifier to validate and register.
/// `identifier` 是需要验证并注册的精确本地协议消息标识。
///
/// # Returns
/// 返回值
/// Returns success after registering a non-empty canonical unique identifier, or the exact identity failure.
/// 注册非空、规范且唯一的标识后返回成功，或返回精确身份失败。
pub(crate) fn register_canonical_message_identifier(
    identifiers: &mut BTreeSet<String>,
    identifier: &str,
) -> Result<(), CanonicalMessageIdentifierError> {
    let canonical = identifier.trim();
    if canonical.is_empty() {
        return Err(CanonicalMessageIdentifierError::Missing);
    }
    if canonical != identifier {
        return Err(CanonicalMessageIdentifierError::NonCanonical {
            identifier: identifier.to_string(),
        });
    }
    if !identifiers.insert(identifier.to_string()) {
        return Err(CanonicalMessageIdentifierError::Duplicate {
            identifier: identifier.to_string(),
        });
    }
    Ok(())
}

/// Describes one invalid canonical conversation-record identifier.
/// 描述一个无效的规范会话记录标识。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionConversationRecordIdentityError {
    /// The record identifier is empty after trimming whitespace.
    /// 记录标识去除首尾空白后为空。
    Empty {
        /// Zero-based position of the invalid record.
        /// 无效记录的零基位置。
        record_index: usize,
    },
    /// The record identifier contains non-canonical surrounding whitespace.
    /// 记录标识包含非规范的首尾空白。
    SurroundingWhitespace {
        /// Zero-based position of the invalid record.
        /// 无效记录的零基位置。
        record_index: usize,
        /// Exact non-canonical identifier stored by the record.
        /// 记录保存的原始非规范标识。
        record_id: String,
    },
    /// The record identifier is already owned by an earlier record.
    /// 记录标识已被更早的记录占用。
    Duplicate {
        /// Zero-based position of the later conflicting record.
        /// 后出现冲突记录的零基位置。
        record_index: usize,
        /// Identifier already owned by an earlier record.
        /// 已被更早记录占用的标识。
        record_id: String,
    },
}

/// Describes one invalid canonical conversation-message identifier.
/// 描述一个无效的规范会话消息标识。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionConversationMessageIdentityError {
    /// The message identifier is empty after trimming whitespace.
    /// 消息标识去除首尾空白后为空。
    Empty {
        /// Zero-based position of the invalid message.
        /// 无效消息的零基位置。
        message_index: usize,
    },
    /// The message identifier contains non-canonical surrounding whitespace.
    /// 消息标识包含非规范的首尾空白。
    SurroundingWhitespace {
        /// Zero-based position of the invalid message.
        /// 无效消息的零基位置。
        message_index: usize,
        /// Exact non-canonical identifier stored by the message.
        /// 消息保存的原始非规范标识。
        message_id: String,
    },
    /// The message identifier is already owned by an earlier message.
    /// 消息标识已被更早的消息占用。
    Duplicate {
        /// Zero-based position of the later conflicting message.
        /// 后出现冲突消息的零基位置。
        message_index: usize,
        /// Identifier already owned by an earlier message.
        /// 已被更早消息占用的标识。
        message_id: String,
    },
}

impl Display for SessionConversationMessageIdentityError {
    /// Formats the identity violation with its exact message position and value.
    /// 使用准确的消息位置和值格式化身份违规信息。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty { message_index } => {
                write!(
                    formatter,
                    "conversation message {message_index} has an empty id"
                )
            }
            Self::SurroundingWhitespace {
                message_index,
                message_id,
            } => write!(
                formatter,
                "conversation message {message_index} has surrounding whitespace in id {message_id:?}"
            ),
            Self::Duplicate {
                message_index,
                message_id,
            } => write!(
                formatter,
                "conversation message {message_index} duplicates id {message_id:?}"
            ),
        }
    }
}

impl std::error::Error for SessionConversationMessageIdentityError {}

impl Display for SessionConversationRecordIdentityError {
    /// Formats the identity violation with its exact record position and value.
    /// 使用准确的记录位置和值格式化身份违规信息。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty { record_index } => {
                write!(
                    formatter,
                    "conversation record {record_index} has an empty id"
                )
            }
            Self::SurroundingWhitespace {
                record_index,
                record_id,
            } => write!(
                formatter,
                "conversation record {record_index} has surrounding whitespace in id {record_id:?}"
            ),
            Self::Duplicate {
                record_index,
                record_id,
            } => write!(
                formatter,
                "conversation record {record_index} duplicates id {record_id:?}"
            ),
        }
    }
}

impl std::error::Error for SessionConversationRecordIdentityError {}

/// Validates that every canonical conversation record has one non-empty, whitespace-free, globally unique identifier.
/// 校验每条规范会话记录均具有非空、无首尾空白且全局唯一的标识。
///
/// # Parameters
/// 参数
/// `records` is the complete canonical record stream whose identity namespace is validated.
/// `records` 是需要校验身份命名空间的完整规范记录流。
///
/// # Returns
/// 返回值
/// Returns success only when every record identifier is canonical and unique within the stream.
/// 仅当流内每个记录标识均规范且唯一时返回成功。
pub fn validate_session_conversation_record_identities(
    records: &[SessionConversationRecord],
) -> Result<(), SessionConversationRecordIdentityError> {
    let mut seen_ids = BTreeSet::new();

    for (record_index, record) in records.iter().enumerate() {
        let canonical_id = record.id.trim();
        if canonical_id.is_empty() {
            return Err(SessionConversationRecordIdentityError::Empty { record_index });
        }
        if canonical_id != record.id {
            return Err(
                SessionConversationRecordIdentityError::SurroundingWhitespace {
                    record_index,
                    record_id: record.id.clone(),
                },
            );
        }
        if !seen_ids.insert(record.id.as_str()) {
            return Err(SessionConversationRecordIdentityError::Duplicate {
                record_index,
                record_id: record.id.clone(),
            });
        }
    }

    Ok(())
}

/// Validates one proposed record identifier against an already canonical record stream without mutating it.
/// 在不修改状态的情况下，针对已经规范的记录流校验一个候选记录标识。
///
/// # Parameters
/// 参数
/// `records` is the current canonical record stream that must remain unchanged on failure.
/// `records` 是失败时必须保持不变的当前规范记录流。
/// `record_id` is the exact identifier proposed for the appended record.
/// `record_id` 是拟追加记录使用的精确标识。
///
/// # Returns
/// 返回值
/// Returns success only when the existing stream and candidate form one canonical unique namespace.
/// 仅当既有记录流与候选项共同组成规范且唯一的命名空间时返回成功。
pub(crate) fn validate_session_conversation_record_append(
    records: &[SessionConversationRecord],
    record_id: &str,
) -> Result<(), SessionConversationRecordIdentityError> {
    validate_session_conversation_record_identities(records)?;
    // record_index is the stable position the candidate will own only after validation succeeds.
    // record_index 是候选项仅在校验成功后才会占用的稳定位置。
    let record_index = records.len();
    // canonical_id separates missing and padded identities without rewriting the caller-owned value.
    // canonical_id 用于区分缺失与带空白身份，且不会改写调用方持有的值。
    let canonical_id = record_id.trim();
    if canonical_id.is_empty() {
        return Err(SessionConversationRecordIdentityError::Empty { record_index });
    }
    if canonical_id != record_id {
        return Err(
            SessionConversationRecordIdentityError::SurroundingWhitespace {
                record_index,
                record_id: record_id.to_string(),
            },
        );
    }
    if records.iter().any(|record| record.id == record_id) {
        return Err(SessionConversationRecordIdentityError::Duplicate {
            record_index,
            record_id: record_id.to_string(),
        });
    }
    Ok(())
}

/// Validates that every canonical conversation message has one non-empty, whitespace-free, globally unique identifier.
/// 校验每条规范会话消息均具有非空、无首尾空白且全局唯一的标识。
///
/// # Parameters
/// 参数
/// `messages` is the complete active message window whose identity namespace is validated.
/// `messages` 是需要校验身份命名空间的完整活动消息窗口。
///
/// # Returns
/// 返回值
/// Returns success only when every message identifier is canonical and unique within the window.
/// 仅当窗口内每个消息标识均规范且唯一时返回成功。
pub fn validate_session_conversation_message_identities(
    messages: &[SessionConversationMessage],
) -> Result<(), SessionConversationMessageIdentityError> {
    // seen_ids owns the single active-window identity namespace and makes the first duplicate deterministic.
    // seen_ids 持有唯一的活动窗口身份命名空间，并使首个重复冲突具备确定性。
    let mut seen_ids = BTreeSet::new();

    for (message_index, message) in messages.iter().enumerate() {
        register_canonical_message_identifier(&mut seen_ids, &message.id).map_err(|error| {
            match error {
                CanonicalMessageIdentifierError::Missing => {
                    SessionConversationMessageIdentityError::Empty { message_index }
                }
                CanonicalMessageIdentifierError::NonCanonical { identifier } => {
                    SessionConversationMessageIdentityError::SurroundingWhitespace {
                        message_index,
                        message_id: identifier,
                    }
                }
                CanonicalMessageIdentifierError::Duplicate { identifier } => {
                    SessionConversationMessageIdentityError::Duplicate {
                        message_index,
                        message_id: identifier,
                    }
                }
            }
        })?;
    }

    Ok(())
}

/// Validates one proposed message identifier against an already canonical active window without mutating it.
/// 在不修改状态的情况下，针对已经规范的活动窗口校验一个候选消息标识。
///
/// # Parameters
/// 参数
/// `messages` is the current active message window that must remain unchanged on failure.
/// `messages` 是失败时必须保持不变的当前活动消息窗口。
/// `message_id` is the exact identifier proposed for the appended message.
/// `message_id` 是拟追加消息使用的精确标识。
///
/// # Returns
/// 返回值
/// Returns success only when the existing window and proposed appended identity form one canonical unique namespace.
/// 仅当既有窗口与候选追加身份共同组成规范且唯一的命名空间时返回成功。
pub(crate) fn validate_session_conversation_message_append(
    messages: &[SessionConversationMessage],
    message_id: &str,
) -> Result<(), SessionConversationMessageIdentityError> {
    validate_session_conversation_message_identities(messages)?;
    // message_index is the stable position the candidate will own only after validation succeeds.
    // message_index 是候选项仅在校验成功后才会占用的稳定位置。
    let message_index = messages.len();
    // canonical_id separates missing and padded identities without rewriting the caller-owned value.
    // canonical_id 用于区分缺失与带空白身份，且不会改写调用方持有的值。
    let canonical_id = message_id.trim();
    if canonical_id.is_empty() {
        return Err(SessionConversationMessageIdentityError::Empty { message_index });
    }
    if canonical_id != message_id {
        return Err(
            SessionConversationMessageIdentityError::SurroundingWhitespace {
                message_index,
                message_id: message_id.to_string(),
            },
        );
    }
    if messages.iter().any(|message| message.id == message_id) {
        return Err(SessionConversationMessageIdentityError::Duplicate {
            message_index,
            message_id: message_id.to_string(),
        });
    }
    Ok(())
}
