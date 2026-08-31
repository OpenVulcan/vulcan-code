//! Shared Provider-source validation and pairing for API semantics and tool records.
//! API 语义与工具记录共用的 Provider 来源校验和配对。

use super::{ActiveApiState, ensure_api_request_field, provider_source_of};
use crate::v1::errors::DomainError;
use crate::v1::event::{ConversationEvent, ProviderSource, ProviderSourceInput};
use crate::v1::ids::Seq;
use crate::v1::projection::provider_visible_fingerprint;

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
pub(super) fn build_provider_source(
    events: &[ConversationEvent],
    covered_until_seq: Seq,
    source_input: &ProviderSourceInput,
) -> Result<ProviderSource, DomainError> {
    let fingerprint_sha256 = provider_visible_fingerprint(events, covered_until_seq)?;
    Ok(ProviderSource {
        provider: source_input.provider.clone(),
        profile: source_input.profile.clone(),
        model: source_input.model.clone(),
        connection_id: source_input.connection_id.clone(),
        message_id: source_input.message_id.clone(),
        response_id: source_input.response_id.clone(),
        replay: source_input.replay.clone(),
        covered_until_seq,
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

/// Requires a previously durable Provider source to equal the terminal typed source completely.
/// 要求此前已可靠落盘的 Provider 来源与终态强类型来源完全一致。
pub(super) fn ensure_provider_source_matches(
    expected: &ProviderSource,
    actual: &ProviderSource,
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
    ensure_api_request_field(
        "provider_source.fingerprint_sha256",
        &expected.fingerprint_sha256,
        &actual.fingerprint_sha256,
    )
}
