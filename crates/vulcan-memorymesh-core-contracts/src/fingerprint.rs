//! Canonical binary request-fingerprint material shared byte-identically by Rust and Go.
//! 由 Rust 与 Go 逐字节共享的规范二进制请求指纹材料。

use sha2::{Digest, Sha256};

/// Names the canonical fingerprint format included in every material prefix.
/// 命名包含在每份材料前缀中的规范指纹格式。
pub const FINGERPRINT_DOMAIN: &str = "vmm-fp-v1";

/// Writes canonical little-endian primitive fields.
/// 写入规范小端序基本字段。
#[derive(Default)]
pub struct CanonicalWriter {
    /// Carries the accumulated material bytes.
    /// 携带累计的材料字节。
    pub buffer: Vec<u8>,
}

impl CanonicalWriter {
    /// Creates an empty canonical writer.
    /// 创建一个空的规范写入器。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes one unsigned 32-bit little-endian integer.
    /// 写入一个无符号 32 位小端整数。
    pub fn u32(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Writes one unsigned 64-bit little-endian integer.
    /// 写入一个无符号 64 位小端整数。
    pub fn u64(&mut self, value: u64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Writes one signed 64-bit little-endian integer.
    /// 写入一个有符号 64 位小端整数。
    pub fn i64(&mut self, value: i64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Writes one 32-bit float through its exact bit pattern.
    /// 通过精确位模式写入一个 32 位浮点数。
    pub fn f32(&mut self, value: f32) {
        self.buffer
            .extend_from_slice(&value.to_bits().to_le_bytes());
    }

    /// Writes one boolean as a single byte.
    /// 将布尔值写为单字节。
    pub fn bool(&mut self, value: bool) {
        self.buffer.push(u8::from(value));
    }

    /// Writes one length-prefixed UTF-8 string.
    /// 写入一个带长度前缀的 UTF-8 字符串。
    pub fn string(&mut self, value: &str) {
        self.u64(u64::try_from(value.len()).unwrap_or(u64::MAX));
        self.buffer.extend_from_slice(value.as_bytes());
    }

    /// Writes one optional tagged value.
    /// 写入一个带标签的可选值。
    pub fn optional<T>(&mut self, value: Option<T>, write: impl Fn(&mut Self, T)) {
        match value {
            None => self.bool(false),
            Some(inner) => {
                self.bool(true);
                write(self, inner);
            }
        }
    }

    /// Writes one count-prefixed ordered collection.
    /// 写入一个带计数前缀的有序集合。
    pub fn list<T>(&mut self, values: &[T], write: impl Fn(&mut Self, &T)) {
        self.u64(u64::try_from(values.len()).unwrap_or(u64::MAX));
        for value in values {
            write(self, value);
        }
    }
}

/// Computes the canonical semantic fingerprint material of one typed command.
/// 计算一个强类型命令的规范语义指纹材料。
///
/// The material excludes mutation identity, observation time, retries, and transport metadata.
/// Explicit business timestamps, including the analysis cutoff, remain part of the semantic payload.
/// 材料排除变更身份、观测时间、重试与传输元数据。
/// 显式业务时间戳（包括分析截止时间）仍属于语义载荷。
#[must_use]
pub fn fingerprint_material(command: &crate::CoreCommand, config_snapshot_id: &str) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    writer.string(FINGERPRINT_DOMAIN);
    writer.u32(command.kind());
    writer.string(command.subject_id().as_str());
    writer.string(command.scope_access().write_scope.as_str());
    writer.list(&command.scope_access().visible_scopes, |inner, scope| {
        inner.string(scope.as_str());
    });
    writer.string(config_snapshot_id);
    write_semantic_payload(&mut writer, command);
    writer.buffer
}

/// Computes the stable `fp256` fingerprint text of one typed command.
/// 计算一个强类型命令的稳定 `fp256` 指纹文本。
#[must_use]
pub fn request_fingerprint(command: &crate::CoreCommand, config_snapshot_id: &str) -> String {
    let material = fingerprint_material(command, config_snapshot_id);
    format!("fp256:{:x}", Sha256::digest(material))
}

/// Validates that the caller-provided fingerprint matches the recomputed semantic fingerprint.
/// 校验调用方提供的指纹与重算的语义指纹一致。
///
/// # Errors
/// Returns a stable identity error when the fingerprint does not match the command material.
/// 当指纹与命令材料不一致时返回稳定身份错误。
pub fn verify_fingerprint(
    command: &crate::CoreCommand,
    config_snapshot_id: &str,
    provided: &str,
) -> Result<(), crate::CoreError> {
    let expected = request_fingerprint(command, config_snapshot_id);
    if provided == expected {
        Ok(())
    } else {
        Err(crate::CoreError::new(
            crate::ErrorCode::MutationIdentityInvalid,
            crate::ErrorStage::Validation,
            "request fingerprint does not match the canonical semantic command material",
        ))
    }
}

/// Writes one command-specific semantic payload.
/// 写入命令专属的语义载荷。
#[allow(clippy::too_many_lines)]
fn write_semantic_payload(writer: &mut CanonicalWriter, command: &crate::CoreCommand) {
    match command {
        crate::CoreCommand::PreCheck(command) => {
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            write_candidate_scope(writer, &command.candidate_scope);
            writer.string(&command.user_content);
            write_precheck_turns(writer, &command.recent_turns);
            writer.u32(command.recall_mode as u32);
            writer.u32(command.session_recall_mode as u32);
            writer.u32(command.max_search_queries);
            writer.u32(command.max_memories);
            writer.i64(command.now_ns);
        }
        crate::CoreCommand::PostActionIntake(command) => {
            writer.u32(command.input_kind.as_u32());
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            write_timeline(writer, &command.timeline);
            writer.i64(command.completed_at_ns);
        }
        crate::CoreCommand::PostActionAnalyzePendingTurn(command) => {
            write_replacement_scope(writer, &command.replacement_scope);
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            writer.string(command.turn_id.as_str());
            writer.u64(command.analysis_generation);
            writer.i64(command.analysis_cutoff_ns);
            writer.i64(command.direct_write_observed_after_ns);
            writer.list(&command.profile_sections, |inner, section| {
                inner.string(section.scope_id.as_str());
                inner.u32(section.role as u32);
                inner.string(&section.display_label);
            });
        }
        crate::CoreCommand::ChatCompact(command) => {
            writer.string(command.session_source_id.as_str());
            writer.i64(command.compacted_at_ns);
        }
        crate::CoreCommand::MemorySearch(command) => {
            writer.string(&command.query);
            writer.u32(command.limit);
            writer.string(command.vector_space_id.as_str());
            write_candidate_boundary(writer, &command.boundary);
            writer.i64(command.now_ns);
        }
        crate::CoreCommand::MemoryDetails(command) => {
            writer.string(command.memory_id.as_str());
        }
        crate::CoreCommand::MemoryWrite(command) => {
            write_replacement_scope(writer, &command.replacement_scope);
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            write_write_batch_item(writer, &command.item);
            writer.i64(command.occurred_at_ns);
        }
        crate::CoreCommand::MemoryDelete(command) => {
            writer.string(command.memory_id.as_str());
            writer.optional(command.expected_version, CanonicalWriter::u64);
        }
        crate::CoreCommand::ProfileQuery(command) => {
            write_profile_view(writer, &command.view);
            writer.i64(command.evaluated_at_ns);
        }
        crate::CoreCommand::ProfileInstruction(command) => {
            write_profile_section(writer, &command.target_section);
            writer.string(&command.instruction);
            writer.i64(command.occurred_at_ns);
        }
        crate::CoreCommand::Retention(command) => {
            write_retention_policy(writer, &command.policy);
            writer.i64(command.now_ns);
        }
        crate::CoreCommand::MemorySearchBatch(command) => {
            writer.list(&command.queries, |inner, query| inner.string(query));
            writer.u32(command.top_k);
            writer.string(command.vector_space_id.as_str());
            write_candidate_boundary(writer, &command.boundary);
            writer.i64(command.now_ns);
        }
        crate::CoreCommand::MemoryDetailsBatch(command) => {
            writer.list(&command.memory_ids, |inner, id| inner.string(id.as_str()));
            writer.list(&command.turn_ids, |inner, id| inner.string(id.as_str()));
        }
        crate::CoreCommand::MemoryWriteBatch(command) => {
            write_replacement_scope(writer, &command.replacement_scope);
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            writer.list(&command.authorized_write_scopes, |inner, scope| {
                inner.string(scope.as_str());
            });
            writer.i64(command.occurred_at_ns);
            writer.list(&command.items, write_write_batch_item);
        }
        crate::CoreCommand::MemoryDeleteBatch(command) => {
            writer.string(&command.reason);
            writer.list(&command.items, |inner, item| {
                inner.string(item.memory_id.as_str());
                inner.optional(item.expected_version, CanonicalWriter::u64);
            });
        }
        crate::CoreCommand::Maintenance(command) => {
            writer.i64(command.occurred_at_ns);
            match &command.action {
                crate::MaintenanceAction::RestoreRecycled(items) => {
                    writer.u32(1);
                    write_versioned_refs(writer, items);
                }
                crate::MaintenanceAction::PurgeRecycled(items) => {
                    writer.u32(2);
                    write_versioned_refs(writer, items);
                }
                crate::MaintenanceAction::RebuildVectors {
                    memory_ids,
                    vector_space_id,
                } => {
                    writer.u32(3);
                    writer.list(memory_ids, |inner, id| inner.string(id.as_str()));
                    writer.string(vector_space_id.as_str());
                }
            }
        }
    }
}

/// Writes one canonical ordered timeline.
/// 写入一个规范化有序时间线。
fn write_timeline(writer: &mut CanonicalWriter, timeline: &[crate::TimelineItem]) {
    writer.list(timeline, |inner, item| {
        inner.u32(item.role as u32);
        inner.string(&item.content);
        inner.i64(item.occurred_at_ns);
    });
}

/// Writes same-session `PreCheck` turn contexts into semantic fingerprint material.
/// 将同会话 `PreCheck` 轮次上下文写入语义指纹材料。
fn write_precheck_turns(writer: &mut CanonicalWriter, turns: &[crate::PreCheckTurnContext]) {
    writer.list(turns, |inner, turn| {
        inner.string(turn.turn_id.as_str());
        inner.u32(turn.content_type as u32);
        inner.string(&turn.content);
    });
}

/// Writes one ordered direct-write batch item.
/// 写入一个有序主动写入批次项。
fn write_write_batch_item(writer: &mut CanonicalWriter, item: &crate::MemoryWriteBatchItem) {
    writer.string(item.target_scope_id.as_str());
    writer.u32(item.scope_class as u32);
    writer.u32(item.category as u32);
    writer.string(&item.summary);
    writer.string(&item.details);
    writer.optional(item.priority, CanonicalWriter::u32);
    writer.optional(
        item.memory_level.map(|level| level as u32),
        CanonicalWriter::u32,
    );
    writer.optional(item.expires_at_ns, CanonicalWriter::i64);
    writer.list(&item.tags, |inner, tag| inner.string(tag));
}

/// Writes one versioned maintenance reference list.
/// 写入一个带版本维护引用列表。
fn write_versioned_refs(writer: &mut CanonicalWriter, items: &[crate::VersionedMemoryRef]) {
    writer.list(items, |inner, item| {
        inner.string(item.memory_id.as_str());
        inner.u64(item.expected_version);
    });
}

/// Writes one complete retention policy snapshot.
/// 写入一个完整的保留策略快照。
fn write_retention_policy(writer: &mut CanonicalWriter, policy: &crate::RetentionPolicy) {
    writer.u32(policy.schema_version);
    writer.u32(policy.turn_hot_window_size);
    writer.u64(policy.session_idle_ms);
    writer.u64(policy.trash_retention_ms);
    writer.u32(policy.protected_priority_floor);
    writer.u32(policy.protected_memory_level_floor);
    writer.bool(policy.skip_protected_shared_memories);
    writer.u32(policy.profile_expiry_limit);
    writer.u32(policy.memory_recycle_limit);
    writer.u32(policy.cold_turn_scan_limit);
    writer.u32(policy.cold_turn_job_limit);
    writer.u32(policy.idle_session_limit);
    writer.u32(policy.vector_gc_limit);
    writer.u32(policy.trash_purge_limit);
}

/// Writes one host-resolved profile view.
/// 写入一个宿主解析画像视图。
fn write_profile_view(writer: &mut CanonicalWriter, view: &crate::ProfileView) {
    writer.u32(view.mode as u32);
    writer.bool(view.include_explanations);
    writer.string(&view.display_timezone);
    writer.list(&view.sections, write_profile_section);
}

/// Writes one host-resolved profile section into a canonical fingerprint.
/// 将一个宿主解析画像分区写入规范指纹。
fn write_profile_section(writer: &mut CanonicalWriter, section: &crate::ProfileViewSection) {
    writer.string(section.scope_id.as_str());
    writer.u32(section.role as u32);
    writer.string(&section.display_label);
}

/// Binds candidate filtering and supersede authority into both request fingerprints and canonical command storage.
/// 将候选过滤和替代授权同时绑定到请求指纹及规范命令存储。
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn write_replacement_scope(
    writer: &mut CanonicalWriter,
    scope: &crate::MemoryReplacementScope,
) {
    write_candidate_scope(writer, &scope.candidates);
    writer.list(&scope.authorized_supersede_scopes, |inner, id| {
        inner.string(id.as_str());
    });
}

/// Writes an exact physical-candidate restriction without granting write authority.
/// 写入精确物理候选限制，但不授予写入权限。
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn write_candidate_scope(
    writer: &mut CanonicalWriter,
    scope: &crate::MemoryCandidateScope,
) {
    writer.list(&scope.scope_ids, |inner, id| inner.string(id.as_str()));
    writer.optional(scope.origin_session_source_id.as_ref(), |inner, id| {
        inner.string(id.as_str());
    });
}

/// Writes optional public search scope and exact-session boundaries in frozen field order.
/// 按冻结字段顺序写入公共搜索的可选作用域及精确会话边界。
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn write_candidate_boundary(
    writer: &mut CanonicalWriter,
    boundary: &crate::CandidateQueryBoundary,
) {
    writer.optional(boundary.candidate_scope.as_ref(), write_candidate_scope);
    writer.optional(
        boundary.session_recall_boundary.as_ref(),
        |inner, session| {
            inner.string(session.session_source_id.as_str());
            inner.u32(session.mode as u32);
        },
    );
}
