//! Profile-driven usage extraction and bounded response lifecycle ordering.
//! 基于 Profile 的用量抽取与有界响应生命周期排序。

use serde_json::Value;
use vulcan_ai_contract::AiStreamEvent;
use vulcan_llm_core::ProtocolConversionError;
use vulcan_llm_core::{UsageAccumulator, UsageExtractionProfile, UsageRecordKind};

/// Maximum profiled usage events retained before the normalized response starts.
/// 规范化响应开始前允许保留的最大 Profile 用量事件数。
const PROFILED_USAGE_GATE_MAX_PENDING_EVENTS: usize = 256;

/// Delays raw profiled usage until the normalized response lifecycle has started.
/// 将原始 Profile 用量延迟到规范化响应生命周期开始之后。
#[derive(Debug, Default)]
pub(super) struct ProfiledUsageEventGate {
    /// Whether a validated response start event has been observed.
    /// 是否已经观察到通过校验的响应开始事件。
    response_started: bool,
    /// Usage events collected before the response start event.
    /// 在响应开始事件之前采集的用量事件。
    pending: Vec<AiStreamEvent>,
}

impl ProfiledUsageEventGate {
    /// Stages newly extracted usage and returns events that may be published immediately.
    /// 暂存新提取的用量并返回可以立即发布的事件。
    ///
    /// # Parameters
    /// 参数
    /// `events` contains one bounded frame's extracted usage observations.
    /// `events` 包含从一个有界帧提取的用量观测。
    ///
    /// # Returns
    /// 返回值
    /// Immediate events after response start, an empty vector while staging, or a bounded overflow error.
    /// 响应开始后返回立即发布事件；暂存期间返回空向量；超过上限时返回有界溢出错误。
    pub(super) fn stage(
        &mut self,
        events: Vec<AiStreamEvent>,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.response_started {
            return Ok(events);
        }
        if self.pending.len().saturating_add(events.len()) > PROFILED_USAGE_GATE_MAX_PENDING_EVENTS
        {
            return Err(ProtocolConversionError::new(format!(
                "Provider usage before response start exceeds the {PROFILED_USAGE_GATE_MAX_PENDING_EVENTS}-event limit."
            )));
        }
        self.pending.extend(events);
        Ok(Vec::new())
    }

    /// Observes one validated normalized event and releases pending usage after response start.
    /// 观察一个已校验的规范化事件并在响应开始后释放暂存用量。
    pub(super) fn observe(&mut self, event: &AiStreamEvent) -> Vec<AiStreamEvent> {
        if !matches!(event, AiStreamEvent::ResponseStarted { .. }) {
            return Vec::new();
        }
        self.response_started = true;
        std::mem::take(&mut self.pending)
    }

    /// Rejects a completed transport that collected usage without a normalized response start.
    /// 拒绝已经采集用量却没有规范化响应开始事件的已结束传输。
    pub(super) fn ensure_released(&self) -> Result<(), ProtocolConversionError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        Err(ProtocolConversionError::new(
            "Provider usage was observed before normalization, but the response start event was never emitted.",
        ))
    }
}

/// Extracts usage from one untouched stream record before protocol normalization.
/// 在协议归一化前从一条未经修改的流记录中提取用量。
pub(super) fn extract_profiled_usage_events(
    profiles: &[UsageExtractionProfile],
    record_kind: UsageRecordKind,
    data: &str,
    accumulator: &mut UsageAccumulator,
) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
    extract_profiled_usage_record_events(profiles, record_kind, data, None, accumulator)
}

/// Extracts usage from one record, reusing its already decoded value when projection produced one.
/// 从一条记录提取用量；若投影已产出解码值则直接复用，避免重复解析。
///
/// `parsed` must be the JSON decoding of `data` when present.
/// `parsed` 存在时必须是 `data` 的 JSON 解码结果。
pub(super) fn extract_profiled_usage_record_events(
    profiles: &[UsageExtractionProfile],
    record_kind: UsageRecordKind,
    data: &str,
    parsed: Option<&Value>,
    accumulator: &mut UsageAccumulator,
) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
    if !profiles
        .iter()
        .any(|profile| profile.record_kind == record_kind)
        || data.trim() == "[DONE]"
    {
        return Ok(Vec::new());
    }
    if let Some(value) = parsed {
        return extract_profiled_usage_value_events(profiles, record_kind, value, accumulator);
    }
    let value: Value = serde_json::from_str(data).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Provider usage record is not valid JSON before normalization: {error}"
        ))
    })?;
    extract_profiled_usage_value_events(profiles, record_kind, &value, accumulator)
}

/// Extracts usage from one already parsed raw provider record.
/// 从一条已解析的原始供应商记录中提取用量。
pub(super) fn extract_profiled_usage_value_events(
    profiles: &[UsageExtractionProfile],
    record_kind: UsageRecordKind,
    value: &Value,
    accumulator: &mut UsageAccumulator,
) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
    let mut events = Vec::new();
    for profile in profiles {
        let Some(observation) = profile
            .extract(record_kind, value)
            .map_err(|error| ProtocolConversionError::new(error.message))?
        else {
            continue;
        };
        let usage = observation.usage.clone();
        if accumulator
            .apply(None, observation)
            .map_err(|error| ProtocolConversionError::new(error.message))?
        {
            events.push(AiStreamEvent::UsageUpdated {
                usage: Box::new(usage),
            });
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
