//! Typed runtime-host ledgers and clone-only compaction projection.
//! 强类型运行时宿主账本与仅克隆压缩投影。

use crate::protocol::{
    validate_session_conversation_message_identities,
    validate_session_conversation_record_identities,
};
use crate::{SessionConversationMessage, SessionConversationPayload, SessionConversationState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{HostMessageEnvelope, HostMessageKind};

/// Producer-reserved record namespace; user text never establishes this ownership.
/// 生产者保留的记录命名空间；用户正文不能建立此归属。
pub const RUNTIME_HOST_RECORD_PREFIX: &str = "runtime-host-record:";
/// Producer-reserved canonical message namespace.
/// 生产者保留的规范消息命名空间。
pub const RUNTIME_HOST_MESSAGE_PREFIX: &str = "runtime-host:";

/// Durable identity and exact replay material for one runtime-host event.
/// 单个运行时宿主事件的可靠身份及精确重放资料。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeHostEventMetadata {
    /// Required current schema version; older incomplete records are rejected.
    /// 必需的当前 Schema 版本；拒绝旧的不完整记录。
    pub version: u32,
    /// Stable producer-owned source identity.
    /// 生产者持有的稳定来源身份。
    pub source_id: String,
    /// Occurrence shared by an update and its expiration marker.
    /// 更新与对应失效标记共享的发生点。
    pub occurrence_id: String,
    /// Monotonic physical event number within the source.
    /// 来源内单调递增的物理事件编号。
    pub occurrence: u64,
    /// Producer-owned content hash used for exact replay comparison.
    /// 用于精确重放比较的生产者内容哈希。
    pub body_hash: String,
    /// Raw envelope event identity without a canonical message prefix.
    /// 不包含规范消息前缀的原始信封事件身份。
    pub event_id: String,
    /// Whether this event expires the referenced occurrence.
    /// 此事件是否使引用的发生点失效。
    pub expires_source: bool,
    /// Exact typed envelope retained for deterministic replay and routed withdrawal notices.
    /// 为确定性重放和按原路由发送撤下通知保留的精确强类型信封。
    pub envelope: HostMessageEnvelope,
    /// Explicit compaction fact permitting the canonical message to be absent.
    /// 允许规范消息缺失的明确压缩事实。
    pub compacted_out: bool,
}

impl RuntimeHostEventMetadata {
    /// Returns the canonical message identity owned by this event.
    /// 返回本事件拥有的规范消息身份。
    pub fn message_id(&self) -> String {
        format!("{RUNTIME_HOST_MESSAGE_PREFIX}{}", self.event_id)
    }

    /// Returns the durable record identity owned by this event.
    /// 返回本事件拥有的可靠记录身份。
    pub fn record_id(&self) -> String {
        format!("{RUNTIME_HOST_RECORD_PREFIX}{}", self.message_id())
    }

    /// Decodes one producer-owned record and validates its exact identity and required fields.
    /// 解码一条生产者记录，并校验其精确身份和必需字段。
    /// `record_id` and `value` come from the same durable row; returns validated metadata or an explicit error.
    /// `record_id` 和 `value` 来自同一可靠行；返回有效元数据或明确错误。
    pub fn decode(record_id: &str, value: Option<&Value>) -> Result<Self, String> {
        // Missing fields are errors; the unreleased contract has no legacy defaulting path.
        // 缺失字段即错误；未发布契约不存在旧格式默认路径。
        let value =
            value.ok_or_else(|| format!("Runtime host record `{record_id}` has no metadata."))?;
        let metadata: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("Invalid runtime host metadata: {error}"))?;
        if metadata.version != 1
            || metadata.record_id() != record_id
            || metadata.occurrence == 0
            || [
                &metadata.source_id,
                &metadata.occurrence_id,
                &metadata.event_id,
                &metadata.body_hash,
            ]
            .iter()
            .any(|field| field.trim().is_empty())
        {
            return Err(format!(
                "Invalid runtime host record identity or schema: `{record_id}`."
            ));
        }
        metadata.envelope.validate()?;
        let expected_kind = if metadata.expires_source {
            HostMessageKind::ContextExpired
        } else {
            HostMessageKind::ContextUpdate
        };
        if metadata.envelope.event_id != metadata.event_id
            || metadata.envelope.kind != expected_kind
        {
            return Err(format!(
                "Runtime host envelope identity or kind disagrees with its ledger: `{record_id}`."
            ));
        }
        Ok(metadata)
    }
}

/// Reads a complete host ledger and validates every message/record pairing without changing state.
/// 读取完整宿主账本并校验所有消息与记录配对，不改变状态。
/// `state` is the exact canonical window; returns ordered metadata or an explicit identity/content error.
/// `state` 是精确规范窗口；返回有序元数据或明确身份、内容错误。
pub fn read_runtime_host_ledger(
    state: &SessionConversationState,
) -> Result<Vec<RuntimeHostEventMetadata>, String> {
    validate_session_conversation_message_identities(&state.messages)
        .map_err(|error| error.to_string())?;
    validate_session_conversation_record_identities(&state.records)
        .map_err(|error| error.to_string())?;
    // Indexed messages avoid rescanning long histories for each ledger entry.
    // 消息索引避免为每条账本记录重复扫描长历史。
    let messages = state
        .messages
        .iter()
        .map(|message| (message.id.as_str(), message))
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::new();
    let mut owned_messages = BTreeSet::new();
    let mut source_sequences = BTreeMap::new();
    let mut occurrence_owners = BTreeMap::new();
    let mut occurrence_kinds = BTreeSet::new();
    for record in &state.records {
        if !record.id.starts_with(RUNTIME_HOST_RECORD_PREFIX) {
            continue;
        }
        let metadata = RuntimeHostEventMetadata::decode(&record.id, record.metadata.as_ref())?;
        let message_id = metadata.message_id();
        if !owned_messages.insert(message_id.clone()) {
            return Err(format!(
                "Duplicate runtime host event `{}`.",
                metadata.event_id
            ));
        }
        let previous = source_sequences.insert(metadata.source_id.clone(), metadata.occurrence);
        if previous.is_some_and(|sequence| sequence >= metadata.occurrence) {
            return Err(format!(
                "Runtime host source `{}` has unordered event numbers.",
                metadata.source_id
            ));
        }
        let owner = occurrence_owners
            .entry(metadata.occurrence_id.clone())
            .or_insert_with(|| metadata.source_id.clone());
        if owner != &metadata.source_id
            || (!metadata.expires_source
                && !occurrence_kinds.insert(metadata.occurrence_id.clone()))
        {
            return Err(format!(
                "Runtime host occurrence `{}` has conflicting ownership.",
                metadata.occurrence_id
            ));
        }
        let rendered = metadata.envelope.render()?;
        match (metadata.compacted_out, messages.get(message_id.as_str())) {
            (true, None) => {}
            (false, Some(message))
                if matches!(&message.payload,
                SessionConversationPayload::UserText {text, attachments}
                    if text == &rendered && attachments.is_empty()) => {}
            _ => {
                return Err(format!(
                    "Runtime host event `{}` has inconsistent message content or compaction state.",
                    metadata.event_id
                ));
            }
        }
        rows.push(metadata);
    }
    for message in &state.messages {
        if message.id.starts_with(RUNTIME_HOST_MESSAGE_PREFIX)
            && !owned_messages.contains(&message.id)
        {
            return Err(format!(
                "Runtime host message `{}` has no matching metadata.",
                message.id
            ));
        }
    }
    // A still-active instruction must never be represented only by an archived ledger entry.
    // 仍有效的指令绝不能只剩一条归档账本记录。
    if active_runtime_host_message_ids(&rows)
        .iter()
        .any(|id| !messages.contains_key(id.as_str()))
    {
        return Err(String::from(
            "An active runtime host instruction is missing from the canonical window.",
        ));
    }
    Ok(rows)
}

/// Selects the latest unexpired update per source without allowing old expiry to invalidate a newer occurrence.
/// 选择每个来源最新且未失效的更新，旧失效事件不能使新发生点失效。
/// `rows` must be validated ledger entries; returns canonical message IDs for active instructions.
/// `rows` 必须是已验证账本条目；返回当前有效指令的规范消息 ID。
pub fn active_runtime_host_message_ids(rows: &[RuntimeHostEventMetadata]) -> BTreeSet<String> {
    // Expiration binds an occurrence, whereas replacement follows update order within a source.
    // 失效绑定发生点，而替代关系遵循来源内的更新顺序。
    let expired = rows
        .iter()
        .filter(|row| row.expires_source)
        .map(|row| row.occurrence_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut latest = BTreeMap::<&str, &RuntimeHostEventMetadata>::new();
    for row in rows.iter().filter(|row| !row.expires_source) {
        if latest
            .get(row.source_id.as_str())
            .is_none_or(|previous| previous.occurrence < row.occurrence)
        {
            latest.insert(row.source_id.as_str(), row);
        }
    }
    latest
        .values()
        .filter(|row| !expired.contains(row.occurrence_id.as_str()))
        .map(|row| row.message_id())
        .collect()
}

/// Builds an isolated compaction input excluding expired or replaced host instructions.
/// 构建隔离的压缩输入，排除失效或被替代的宿主指令。
/// `state` is unchanged; returns a clone with exact native messages and a preserved identity ledger.
/// `state` 保持不变；返回保留精确原生消息和身份账本的克隆。
pub fn prepare_runtime_host_compaction(
    state: &SessionConversationState,
) -> Result<SessionConversationState, String> {
    rebuild_runtime_host_compaction(state, state.messages.clone())
}

/// Rebuilds a new context generation while preserving active host instructions and archiving removed events.
/// 重建新的上下文代次，保留有效宿主指令并归档已移出的事件。
/// `source` owns the validated ledger; `compacted_messages` is the checked summary and retained native tail.
/// `source` 持有已验证账本；`compacted_messages` 是已核验的摘要与保留原生尾段。
/// Returns an atomic candidate or an error; neither argument's source state is mutated.
/// 返回原子候选或错误；不会修改来源状态。
pub fn rebuild_runtime_host_compaction(
    source: &SessionConversationState,
    mut compacted_messages: Vec<SessionConversationMessage>,
) -> Result<SessionConversationState, String> {
    let mut ledger = read_runtime_host_ledger(source)?;
    let active_ids = active_runtime_host_message_ids(&ledger);
    let host_ids = ledger
        .iter()
        .map(RuntimeHostEventMetadata::message_id)
        .collect::<BTreeSet<_>>();
    compacted_messages
        .retain(|message| !host_ids.contains(&message.id) || active_ids.contains(&message.id));
    let retained_ids = compacted_messages
        .iter()
        .map(|message| message.id.clone())
        .collect::<BTreeSet<_>>();
    // Backfill only exact still-active messages, in their original order, at the new generation boundary.
    // 仅在新代次边界按原顺序回填仍有效的精确消息。
    let mut backfill = source
        .messages
        .iter()
        .filter(|message| active_ids.contains(&message.id) && !retained_ids.contains(&message.id))
        .cloned()
        .collect::<Vec<_>>();
    backfill.extend(compacted_messages);
    let final_ids = backfill
        .iter()
        .map(|message| message.id.clone())
        .collect::<BTreeSet<_>>();
    if !active_ids.is_subset(&final_ids) {
        return Err(String::from(
            "An active runtime host instruction has no canonical message to preserve.",
        ));
    }
    let mut candidate = source.clone();
    candidate.messages = backfill;
    // Rewrite only ledger compaction facts; original body, routing, source and occurrence stay immutable.
    // 仅改写账本压缩事实；原正文、路由、来源和发生点保持不变。
    let mut records = candidate
        .records
        .iter_mut()
        .map(|record| (record.id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    for metadata in &mut ledger {
        metadata.compacted_out = !final_ids.contains(&metadata.message_id());
        let record = records.get_mut(&metadata.record_id()).ok_or_else(|| {
            String::from("Runtime host ledger lost its record during compaction.")
        })?;
        record.metadata = Some(serde_json::to_value(&metadata).map_err(|error| error.to_string())?);
    }
    candidate.continuation = Default::default();
    read_runtime_host_ledger(&candidate)?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionConversationRecord, SessionMessageKind, SessionMessageRecordKind};

    /// Builds one valid active instruction ledger and its exact canonical message for corruption checks.
    /// 为损坏校验构造一个有效指令账本及其精确规范消息。
    fn active_state() -> SessionConversationState {
        // The ledger is produced from an exact host envelope, never extracted from user text.
        // 账本由精确宿主信封生成，绝不从用户正文提取。
        let metadata = RuntimeHostEventMetadata {
            version: 1,
            source_id: String::from("source"),
            occurrence_id: String::from("occ"),
            occurrence: 1,
            body_hash: String::from("hash"),
            event_id: String::from("event"),
            expires_source: false,
            envelope: HostMessageEnvelope {
                event_id: String::from("event"),
                kind: HostMessageKind::ContextUpdate,
                source: String::from("source"),
                target: String::from("model"),
                task_id: None,
                question_id: None,
                scope: String::from("session"),
                body: String::from("exact host envelope"),
            },
            compacted_out: false,
        };
        SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: metadata.message_id(),
                payload: SessionConversationPayload::UserText {
                    text: metadata.envelope.render().unwrap(),
                    attachments: Vec::new(),
                },
            }],
            records: vec![SessionConversationRecord {
                id: metadata.record_id(),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::ExploreOutput,
                visible: false,
                title: String::new(),
                body: String::from("raw body"),
                metadata: Some(serde_json::to_value(metadata).unwrap()),
                created_at: String::from("0"),
            }],
            ..Default::default()
        }
    }

    /// Rejects missing, forged, reordered and inconsistent ledger state without modifying any source bytes.
    /// 拒绝缺失、伪造、乱序及不一致账本状态，不改变来源字节。
    #[test]
    fn compaction_rejects_invalid_host_ledger_atomically() {
        let valid = active_state();
        assert!(read_runtime_host_ledger(&valid).is_ok());
        // Each independent corruption must fail explicitly, rather than being treated as a user message.
        // 每种独立损坏都必须明确失败，不能作为普通用户消息放行。
        for case in 0..10 {
            let mut state = valid.clone();
            match case {
                0 => state.records[0].metadata = None,
                1 => state.records[0].metadata.as_mut().unwrap()["version"] = 99.into(),
                2 => state.records[0].id.push_str("-wrong"),
                3 => state.messages.clear(),
                4 => state.records[0].metadata.as_mut().unwrap()["compacted_out"] = true.into(),
                5 => {
                    state.records[0].metadata.as_mut().unwrap()["envelope"]["body"] =
                        "changed".into()
                }
                6 => {
                    state.messages.clear();
                    state.records[0].metadata.as_mut().unwrap()["compacted_out"] = true.into();
                }
                7 => state.records.clear(),
                8 => {
                    state.records[0].metadata.as_mut().unwrap()["envelope"]["event_id"] =
                        "wrong-event".into()
                }
                9 => {
                    state.records[0].metadata.as_mut().unwrap()["envelope"]["kind"] =
                        "task_start".into()
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_value(&state).unwrap();
            assert!(
                prepare_runtime_host_compaction(&state).is_err(),
                "case {case}"
            );
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        }
    }

    /// Rejects an invalid replacement without rewriting the original ledger or duplicating an active backfill.
    /// 拒绝无效替换，不改写原账本，也不重复有效回填。
    #[test]
    fn compaction_backfill_preserves_exact_identity_and_atomic_errors() {
        let state = active_state();
        let before = serde_json::to_value(&state).unwrap();
        let rebuilt = rebuild_runtime_host_compaction(&state, Vec::new()).unwrap();
        assert_eq!(serde_json::to_value(&rebuilt).unwrap(), before);
        let mut altered = state.messages.clone();
        if let SessionConversationPayload::UserText { text, .. } = &mut altered[0].payload {
            *text = String::from("altered host");
        }
        assert!(rebuild_runtime_host_compaction(&state, altered).is_err());
        let duplicated = [state.messages.clone(), state.messages.clone()].concat();
        assert!(rebuild_runtime_host_compaction(&state, duplicated).is_err());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}
