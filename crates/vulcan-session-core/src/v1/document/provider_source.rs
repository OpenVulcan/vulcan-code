//! Shared Provider-source validation and pairing for API semantics and tool records.
//! API 语义与工具记录共用的 Provider 来源校验和配对。

use super::{ActiveApiState, SessionDocument, ensure_api_request_field, provider_source_of};
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ConversationEvent, EventBody, ProviderFingerprintVersion, ProviderSource, ProviderSourceInput,
};
use crate::v1::ids::Seq;
use crate::v1::projection::{
    provider_fingerprint_mismatches, provider_visible_fingerprint_of, validate_provider_fingerprint,
};
use std::collections::BTreeMap;

/// Validates one typed Provider source against the immutable active API route.
/// 校验一份强类型 Provider 来源是否匹配不可变活动 API 路由。
pub(super) fn validate_provider_source_input(
    api: &ActiveApiState,
    source_input: &ProviderSourceInput,
) -> Result<(), DomainError> {
    ensure_api_request_field(
        "provider_source.provider",
        &api.request.provider,
        &source_input.provider,
    )?;
    ensure_api_request_field(
        "provider_source.profile",
        &api.request.profile,
        &source_input.profile,
    )?;
    ensure_api_request_field(
        "provider_source.model",
        &api.request.model,
        &source_input.model,
    )?;
    ensure_api_request_field(
        "provider_source.connection_id",
        &api.request.connection_id,
        &source_input.connection_id,
    )?;
    if let Some(replay) = &source_input.replay {
        ensure_api_request_field(
            "provider_source.replay.provider_id",
            &source_input.provider,
            &replay.provider_id,
        )?;
        ensure_api_request_field(
            "provider_source.replay.connection_id",
            &source_input.connection_id.as_deref().unwrap_or_default(),
            &replay.connection_id.as_str(),
        )?;
        ensure_api_request_field(
            "provider_source.replay.protocol_profile_id",
            &source_input.profile.as_deref().unwrap_or_default(),
            &replay.protocol_profile_id.as_str(),
        )?;
        ensure_api_request_field(
            "provider_source.replay.model_id",
            &source_input.model,
            &replay.model_id,
        )?;
    }
    Ok(())
}

/// Builds the authoritative Provider source for one document-selected coverage boundary.
/// 为文档选定的覆盖边界构建权威 Provider 来源。
/// `events` is the checkpoint's fingerprint window in seq order (see `SessionDocument::fingerprint_window`).
/// `events` 为检查点按 seq 排序的指纹窗口(见 `SessionDocument::fingerprint_window`)。
pub(super) fn build_provider_source<'a>(
    events: impl IntoIterator<Item = &'a ConversationEvent>,
    covered_until_seq: Seq,
    source_input: &ProviderSourceInput,
) -> Result<ProviderSource, DomainError> {
    let fingerprint_sha256 = provider_visible_fingerprint_of(events, covered_until_seq)?;
    Ok(ProviderSource {
        provider: source_input.provider.clone(),
        profile: source_input.profile.clone(),
        model: source_input.model.clone(),
        connection_id: source_input.connection_id.clone(),
        message_id: source_input.message_id.clone(),
        response_id: source_input.response_id.clone(),
        replay: source_input.replay.clone(),
        covered_until_seq,
        fingerprint_version: ProviderFingerprintVersion::ImmutableProjectionV1,
        fingerprint_sha256,
    })
}

/// Compares one persisted Provider source with its original typed input.
/// 将一条已持久化 Provider 来源与其原始强类型输入进行比较。
pub(super) fn provider_source_matches_input(
    source: &ProviderSource,
    input: &ProviderSourceInput,
) -> bool {
    source.provider == input.provider
        && source.profile == input.profile
        && source.model == input.model
        && source.connection_id == input.connection_id
        && source.message_id == input.message_id
        && source.response_id == input.response_id
        && source.replay == input.replay
}

/// Returns the exact Provider source hosted by one resident event sequence.
/// 返回一个常驻事件序列承载的精确 Provider 来源。
pub(super) fn provider_source_at_seq(
    events: &[ConversationEvent],
    resident_start_seq: u64,
    seq: Seq,
) -> Result<ProviderSource, DomainError> {
    let index =
        seq.value()
            .checked_sub(resident_start_seq)
            .ok_or(DomainError::ImmutableFieldMutation {
                entity: "events",
                field: "sealed_segment",
            })? as usize;
    events
        .get(index)
        .and_then(provider_source_of)
        .ok_or_else(|| DomainError::reference("provider_source", seq.value()))
}

/// Requires matching route/coverage and verifies the durable digest under its original format.
/// 要求路由、覆盖范围一致，并按原始格式验证持久化摘要。
/// `events` supplies immutable facts; successful verification never upgrades an existing checkpoint.
/// `events` 提供不可变事实；校验成功绝不升级既有检查点。
pub(super) fn ensure_provider_source_matches(
    expected: &ProviderSource,
    actual: &ProviderSource,
    events: &[ConversationEvent],
    later_window_starts: &[Seq],
) -> Result<(), DomainError> {
    ensure_api_request_field(
        "provider_source.provider",
        &expected.provider,
        &actual.provider,
    )?;
    ensure_api_request_field(
        "provider_source.profile",
        &expected.profile,
        &actual.profile,
    )?;
    ensure_api_request_field("provider_source.model", &expected.model, &actual.model)?;
    ensure_api_request_field(
        "provider_source.connection_id",
        &expected.connection_id,
        &actual.connection_id,
    )?;
    ensure_api_request_field(
        "provider_source.message_id",
        &expected.message_id,
        &actual.message_id,
    )?;
    ensure_api_request_field(
        "provider_source.response_id",
        &expected.response_id,
        &actual.response_id,
    )?;
    ensure_api_request_field("provider_source.replay", &expected.replay, &actual.replay)?;
    ensure_api_request_field(
        "provider_source.covered_until_seq",
        &expected.covered_until_seq,
        &actual.covered_until_seq,
    )?;
    // A checkpoint loaded before API completion retains its original hash contract.
    // 在 API 完成前重载的检查点保留原始哈希契约。
    validate_provider_fingerprint(events, later_window_starts, actual)
}

impl SessionDocument {
    /// Segment starts after the resident start; a resident checkpoint digest may cover only from one of them.
    /// 常驻起点之后的分段起点；常驻检查点摘要可能只覆盖其中某个起点之后的事件。
    pub(super) fn later_fingerprint_window_starts(&self) -> Vec<Seq> {
        self.segment_catalog
            .iter()
            .map(|entry| entry.start_seq)
            .filter(|start| start.value() > self.resident_start_seq)
            .collect()
    }

    /// Resident events a new checkpoint digest covers: from the latest known segment start at or before it.
    /// 新检查点摘要覆盖的常驻事件：从不晚于它的最新已知分段起点开始。
    ///
    /// An open API group never rolls over into a new segment, so every later resident window that holds the
    /// checkpoint also holds this start, and verification never needs history the window does not load.
    /// 开放的 API 组绝不会跨越分段滚动，因此之后任何包含该检查点的常驻窗口都包含这个起点，
    /// 校验永远不需要窗口之外的历史。
    pub(super) fn fingerprint_window(&self, covered_until_seq: Seq) -> &[ConversationEvent] {
        let start = self
            .segment_catalog
            .iter()
            .map(|entry| entry.start_seq.value())
            .filter(|start| *start <= covered_until_seq.value())
            .max()
            .unwrap_or(0)
            .max(self.resident_start_seq);
        let from = usize::try_from(start - self.resident_start_seq)
            .unwrap_or(usize::MAX)
            .min(self.events.len());
        &self.events[from..]
    }

    /// Verifies every resident checkpoint digest in one batch.
    /// 一次批量校验全部常驻检查点摘要。
    ///
    /// # Returns
    /// 返回值
    /// Hosting seq to `(stored, expected)` for every checkpoint no candidate window reproduces.
    /// 每个无任何候选窗口能复现的检查点：承载 seq 到 `(stored, expected)` 的映射。
    pub(super) fn resident_fingerprint_mismatches(
        &self,
    ) -> Result<BTreeMap<u64, (String, String)>, DomainError> {
        let hosted: Vec<(u64, &ProviderSource)> = self
            .events
            .iter()
            .filter_map(|event| {
                let source = match &event.event {
                    EventBody::AssistantReasoning(reasoning) => reasoning.provider_source.as_ref(),
                    EventBody::AssistantText(text) => text.provider_source.as_ref(),
                    EventBody::ToolCall(tool) => tool.provider_source.as_ref(),
                    _ => None,
                };
                source.map(|source| (event.seq.value(), source))
            })
            .collect();
        let sources: Vec<&ProviderSource> = hosted.iter().map(|(_, source)| *source).collect();
        let expected = provider_fingerprint_mismatches(
            &self.events,
            &self.later_fingerprint_window_starts(),
            &sources,
        )?;
        Ok(hosted
            .iter()
            .zip(expected)
            .filter_map(|((seq, source), expected)| {
                expected.map(|expected| (*seq, (source.fingerprint_sha256.clone(), expected)))
            })
            .collect())
    }
}
