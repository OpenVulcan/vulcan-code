// The binary codec uses deliberate wildcard imports for the closed command enum and
// repetitive field closures; the reader helpers are inherently constructor-free.
// 二进制编解码器对封闭命令枚举使用刻意通配导入，并重复使用字段闭包；
// 读取辅助函数天然不需要构造函数。
#![allow(
    clippy::enum_glob_use,
    clippy::missing_const_for_fn,
    clippy::redundant_closure_for_method_calls
)]

/// Creates one canonical codec format error.
/// 创建一个规范编解码器格式错误。
fn codec_error(diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(
        crate::ErrorCode::InvalidBinaryFormat,
        crate::ErrorStage::Validation,
        diagnostic,
    )
}

use crate::{CoreCommand, CoreError};

/// Reads canonical little-endian primitive fields written by `CanonicalWriter`.
/// 读取由 `CanonicalWriter` 写入的规范小端序基本字段。
pub struct CanonicalReader<'a> {
    /// Carries the remaining material bytes.
    /// 携带剩余的材料字节。
    bytes: &'a [u8],
    /// Carries the current read position.
    /// 携带当前读取位置。
    position: usize,
}

impl<'a> CanonicalReader<'a> {
    /// Creates one reader over the exact material bytes.
    /// 创建一个覆盖精确材料字节的读取器。
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    /// Returns one stable error when the remaining material is truncated.
    /// 当剩余材料被截断时返回一个稳定错误。
    fn truncated() -> CoreError {
        codec_error("canonical command material is truncated")
    }

    /// Reads one unsigned 32-bit little-endian integer.
    /// 读取一个无符号 32 位小端整数。
    pub fn u32(&mut self) -> Result<u32, CoreError> {
        let end = self.position.checked_add(4).ok_or_else(Self::truncated)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(Self::truncated)?;
        self.position = end;
        Ok(u32::from_le_bytes(
            bytes.try_into().map_err(|_| Self::truncated())?,
        ))
    }

    /// Reads one unsigned 64-bit little-endian integer.
    /// 读取一个无符号 64 位小端整数。
    pub fn u64(&mut self) -> Result<u64, CoreError> {
        let end = self.position.checked_add(8).ok_or_else(Self::truncated)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(Self::truncated)?;
        self.position = end;
        Ok(u64::from_le_bytes(
            bytes.try_into().map_err(|_| Self::truncated())?,
        ))
    }

    /// Reads one signed 64-bit little-endian integer.
    /// 读取一个有符号 64 位小端整数。
    pub fn i64(&mut self) -> Result<i64, CoreError> {
        Ok(i64::from_le_bytes(self.u64()?.to_le_bytes()))
    }

    /// Reads one boolean as a single byte.
    /// 将布尔值读为单字节。
    pub fn bool(&mut self) -> Result<bool, CoreError> {
        match self.bytes.get(self.position) {
            Some(0) => {
                self.position += 1;
                Ok(false)
            }
            Some(1) => {
                self.position += 1;
                Ok(true)
            }
            _ => Err(Self::truncated()),
        }
    }

    /// Reads one length-prefixed UTF-8 string.
    /// 读取一个带长度前缀的 UTF-8 字符串。
    pub fn string(&mut self) -> Result<String, CoreError> {
        let length = usize::try_from(self.u64()?).map_err(|_| Self::truncated())?;
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(Self::truncated)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(Self::truncated)?;
        self.position = end;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| codec_error("canonical command string is not UTF-8"))
    }

    /// Reads one tagged optional value.
    /// 读取一个带标签的可选值。
    pub fn optional<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T, CoreError>,
    ) -> Result<Option<T>, CoreError> {
        if self.bool()? {
            read(self).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Reads one count-prefixed ordered collection.
    /// 读取一个带计数前缀的有序集合。
    pub fn list<T>(
        &mut self,
        read: impl FnMut(&mut Self) -> Result<T, CoreError>,
    ) -> Result<Vec<T>, CoreError> {
        let count = usize::try_from(self.u64()?).map_err(|_| Self::truncated())?;
        // Every encoded canonical item consumes at least one byte; a count cannot allocate ahead of its payload.
        // 每个规范编码元素至少消耗一个字节；计数不得在其载荷之前驱动大规模分配。
        if count > self.bytes.len().saturating_sub(self.position) {
            return Err(Self::truncated());
        }
        let mut values = Vec::with_capacity(count.min(1_024));
        let mut reader = read;
        for _ in 0..count {
            values.push(reader(self)?);
        }
        Ok(values)
    }

    /// Reports whether the entire material was consumed.
    /// 报告是否已消费全部材料。
    pub fn is_exhausted(&self) -> bool {
        self.position == self.bytes.len()
    }
}

/// Encodes one public command into its canonical binary form for checkpoint storage.
/// 将一个公共命令编码为用于检查点存储的规范二进制形式。
///
/// The format writes the kind discriminant followed by every semantic field and the
/// persistent mutation identity, so `decode_command` reconstructs the exact input.
/// 格式写入类型判别值后跟每个语义字段与持久变更身份，使 `decode_command` 能重建精确输入。
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn encode_command(command: &CoreCommand) -> Vec<u8> {
    let mut writer = crate::CanonicalWriter::new();
    writer.u32(u32::from_le_bytes(*b"VMCI"));
    writer.u32(2);
    writer.u32(command.kind());
    write_command_payload(&mut writer, command);
    writer.buffer
}

/// Decodes one canonical command back into its exact typed form.
/// 将一个规范命令解码回其精确强类型形式。
///
/// # Errors
/// Returns a stable format error when the material is truncated or structurally invalid.
/// 当材料被截断或结构无效时返回稳定格式错误。
pub fn decode_command(bytes: &[u8]) -> Result<CoreCommand, CoreError> {
    let mut reader = CanonicalReader::new(bytes);
    if reader.u32()? != u32::from_le_bytes(*b"VMCI") || reader.u32()? != 2 {
        return Err(codec_error("unsupported canonical command envelope"));
    }
    let kind = reader.u32()?;
    let command = read_command_payload(&mut reader, kind)?;
    if !reader.is_exhausted() {
        return Err(codec_error("canonical command material has trailing bytes"));
    }
    Ok(command)
}

/// Writes one command-specific payload for the canonical checkpoint codec.
/// 为规范检查点编解码器写入命令专属载荷。
#[allow(clippy::too_many_lines)]
fn write_command_payload(writer: &mut crate::CanonicalWriter, command: &CoreCommand) {
    use crate::CoreCommand::*;
    match command {
        PreCheck(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            crate::fingerprint::write_candidate_scope(writer, &command.candidate_scope);
            writer.string(&command.user_content);
            write_precheck_turns(writer, &command.recent_turns);
            writer.u32(command.recall_mode as u32);
            writer.u32(command.session_recall_mode as u32);
            writer.u32(command.max_search_queries);
            writer.u32(command.max_memories);
            writer.i64(command.now_ns);
            write_identity(writer, &command.mutation_identity);
        }
        PostActionIntake(command) => {
            writer.u32(command.input_kind.as_u32());
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            write_timeline_list(writer, &command.timeline);
            writer.i64(command.completed_at_ns);
            write_identity(writer, &command.mutation_identity);
        }
        PostActionAnalyzePendingTurn(command) => {
            crate::fingerprint::write_replacement_scope(writer, &command.replacement_scope);
            write_scope_access(writer, &command.scope_access);
            write_profile_sections(writer, &command.profile_sections);
            writer.string(command.subject_id.as_str());
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            writer.string(command.turn_id.as_str());
            writer.u64(command.analysis_generation);
            writer.i64(command.analysis_cutoff_ns);
            writer.i64(command.direct_write_observed_after_ns);
            write_identity(writer, &command.mutation_identity);
        }
        ChatCompact(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.session_source_id.as_str());
            writer.i64(command.compacted_at_ns);
            write_identity(writer, &command.mutation_identity);
        }
        MemorySearch(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(&command.query);
            writer.u32(command.limit);
            writer.string(command.vector_space_id.as_str());
            crate::fingerprint::write_candidate_boundary(writer, &command.boundary);
            writer.i64(command.now_ns);
        }
        MemoryDetails(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.memory_id.as_str());
        }
        MemoryWrite(command) => {
            crate::fingerprint::write_replacement_scope(writer, &command.replacement_scope);
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            write_memory_write_item(writer, &command.item);
            writer.i64(command.occurred_at_ns);
            write_identity(writer, &command.mutation_identity);
        }
        MemoryDelete(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.memory_id.as_str());
            writer.optional(command.expected_version, crate::CanonicalWriter::u64);
            write_identity(writer, &command.mutation_identity);
        }
        ProfileQuery(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.u32(command.view.mode as u32);
            writer.bool(command.view.include_explanations);
            writer.string(&command.view.display_timezone);
            writer.list(&command.view.sections, |inner, section| {
                inner.string(section.scope_id.as_str());
                inner.u32(section.role as u32);
                inner.string(&section.display_label);
            });
            writer.i64(command.evaluated_at_ns);
        }
        ProfileInstruction(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.target_section.scope_id.as_str());
            writer.u32(command.target_section.role as u32);
            writer.string(&command.target_section.display_label);
            writer.string(&command.instruction);
            writer.i64(command.occurred_at_ns);
            write_identity(writer, &command.mutation_identity);
        }
        Retention(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.i64(command.now_ns);
            write_retention_policy(writer, &command.policy);
            write_identity(writer, &command.mutation_identity);
        }
        MemorySearchBatch(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.list(&command.queries, |inner, query| inner.string(query));
            writer.u32(command.top_k);
            writer.string(command.vector_space_id.as_str());
            crate::fingerprint::write_candidate_boundary(writer, &command.boundary);
            writer.i64(command.now_ns);
        }
        MemoryDetailsBatch(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.list(&command.memory_ids, |inner, id| inner.string(id.as_str()));
            writer.list(&command.turn_ids, |inner, id| inner.string(id.as_str()));
        }
        MemoryWriteBatch(command) => {
            crate::fingerprint::write_replacement_scope(writer, &command.replacement_scope);
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(command.session_source_id.as_str());
            writer.string(command.vector_space_id.as_str());
            writer.list(&command.authorized_write_scopes, |inner, scope| {
                inner.string(scope.as_str());
            });
            writer.i64(command.occurred_at_ns);
            writer.list(&command.items, write_memory_write_item);
            write_identity(writer, &command.mutation_identity);
        }
        MemoryDeleteBatch(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
            writer.string(&command.reason);
            writer.list(&command.items, |inner, item| {
                inner.string(item.memory_id.as_str());
                inner.optional(item.expected_version, crate::CanonicalWriter::u64);
            });
            write_identity(writer, &command.mutation_identity);
        }
        Maintenance(command) => {
            write_scope_access(writer, &command.scope_access);
            writer.string(command.subject_id.as_str());
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
            write_identity(writer, &command.mutation_identity);
        }
    }
}

/// Reads one command-specific payload for the canonical checkpoint codec.
/// 为规范检查点编解码器读取命令专属载荷。
#[allow(clippy::too_many_lines)]
fn read_command_payload(
    reader: &mut CanonicalReader<'_>,
    kind: u32,
) -> Result<CoreCommand, CoreError> {
    use crate::CoreCommand::*;
    Ok(match kind {
        1 => PreCheck(crate::PreCheckCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            session_source_id: crate::SessionSourceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            candidate_scope: read_candidate_scope(reader)?,
            user_content: reader.string()?,
            recent_turns: read_precheck_turns(reader)?,
            recall_mode: crate::RecallMode::try_from_u32(reader.u32()?)
                .ok_or_else(|| codec_error("unknown recall mode"))?,
            session_recall_mode: crate::SessionRecallMode::try_from_u32(reader.u32()?)
                .ok_or_else(|| codec_error("unknown session recall mode"))?,
            max_search_queries: reader.u32()?,
            max_memories: reader.u32()?,
            now_ns: reader.i64()?,
            mutation_identity: read_identity(reader)?,
        }),
        2 => PostActionIntake(crate::PostActionIntakeCommand {
            input_kind: crate::PostActionInputKind::try_from_u32(reader.u32()?)
                .ok_or_else(|| codec_error("unknown PostAction input kind"))?,
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            session_source_id: crate::SessionSourceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            timeline: read_timeline_list(reader)?,
            completed_at_ns: reader.i64()?,
            mutation_identity: read_identity(reader)?,
        }),
        3 => PostActionAnalyzePendingTurn(crate::PostActionAnalyzePendingTurnCommand {
            replacement_scope: read_replacement_scope(reader)?,
            scope_access: read_scope_access(reader)?,
            profile_sections: read_profile_sections(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            session_source_id: crate::SessionSourceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            turn_id: crate::TurnId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            analysis_generation: reader.u64()?,
            analysis_cutoff_ns: reader.i64()?,
            direct_write_observed_after_ns: reader.i64()?,
            mutation_identity: read_identity(reader)?,
        }),
        4 => ChatCompact(crate::ChatCompactCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            session_source_id: crate::SessionSourceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            compacted_at_ns: reader.i64()?,
            mutation_identity: read_identity(reader)?,
        }),
        5 => MemorySearch(crate::MemorySearchCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            query: reader.string()?,
            limit: reader.u32()?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            boundary: read_candidate_boundary(reader)?,
            now_ns: reader.i64()?,
        }),
        6 => MemoryDetails(crate::MemoryDetailsCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            memory_id: crate::MemoryId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
        }),
        7 => MemoryWrite(crate::MemoryWriteCommand {
            replacement_scope: read_replacement_scope(reader)?,
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            session_source_id: crate::SessionSourceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            item: read_memory_write_item(reader)?,
            occurred_at_ns: reader.i64()?,
            mutation_identity: read_identity(reader)?,
        }),
        8 => MemoryDelete(crate::MemoryDeleteCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            memory_id: crate::MemoryId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            expected_version: reader.optional(|inner| inner.u64())?,
            mutation_identity: read_identity(reader)?,
        }),
        9 => ProfileQuery(crate::ProfileQueryCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            view: crate::ProfileView {
                mode: crate::ProfileViewMode::try_from_u32(reader.u32()?)
                    .ok_or_else(|| codec_error("unknown profile view mode"))?,
                include_explanations: reader.bool()?,
                display_timezone: reader.string()?,
                sections: reader.list(|inner| {
                    Ok(crate::ProfileViewSection {
                        scope_id: crate::ScopeId::new(&inner.string()?)
                            .map_err(|error| codec_error(error.to_string()))?,
                        role: crate::SectionRole::try_from_u32(inner.u32()?)
                            .ok_or_else(|| codec_error("unknown section role"))?,
                        display_label: inner.string()?,
                    })
                })?,
            },
            evaluated_at_ns: reader.i64()?,
        }),
        10 => ProfileInstruction(crate::ProfileInstructionCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            target_section: crate::ProfileViewSection {
                scope_id: crate::ScopeId::new(&reader.string()?)
                    .map_err(|error| codec_error(error.to_string()))?,
                role: crate::SectionRole::try_from_u32(reader.u32()?)
                    .ok_or_else(|| codec_error("unknown section role"))?,
                display_label: reader.string()?,
            },
            instruction: reader.string()?,
            occurred_at_ns: reader.i64()?,
            mutation_identity: read_identity(reader)?,
        }),
        11 => Retention(crate::RetentionCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            now_ns: reader.i64()?,
            policy: read_retention_policy(reader)?,
            mutation_identity: read_identity(reader)?,
        }),
        12 => MemorySearchBatch(crate::MemorySearchBatchCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            queries: reader.list(|inner| inner.string())?,
            top_k: reader.u32()?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            boundary: read_candidate_boundary(reader)?,
            now_ns: reader.i64()?,
        }),
        13 => MemoryDetailsBatch(crate::MemoryDetailsBatchCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            memory_ids: reader.list(|inner| {
                crate::MemoryId::new(&inner.string()?)
                    .map_err(|error| codec_error(error.to_string()))
            })?,
            turn_ids: reader.list(|inner| {
                crate::TurnId::new(&inner.string()?).map_err(|error| codec_error(error.to_string()))
            })?,
        }),
        14 => MemoryWriteBatch(crate::MemoryWriteBatchCommand {
            replacement_scope: read_replacement_scope(reader)?,
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            session_source_id: crate::SessionSourceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            authorized_write_scopes: reader.list(|inner| {
                crate::ScopeId::new(&inner.string()?)
                    .map_err(|error| codec_error(error.to_string()))
            })?,
            occurred_at_ns: reader.i64()?,
            items: reader.list(read_memory_write_item)?,
            mutation_identity: read_identity(reader)?,
        }),
        15 => MemoryDeleteBatch(crate::MemoryDeleteBatchCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            reason: reader.string()?,
            items: reader.list(|inner| {
                Ok(crate::MemoryDeleteBatchItem {
                    memory_id: crate::MemoryId::new(&inner.string()?)
                        .map_err(|error| codec_error(error.to_string()))?,
                    expected_version: inner.optional(|reader_inner| reader_inner.u64())?,
                })
            })?,
            mutation_identity: read_identity(reader)?,
        }),
        16 => Maintenance(crate::MaintenanceCommand {
            scope_access: read_scope_access(reader)?,
            subject_id: crate::SubjectId::new(&reader.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            occurred_at_ns: reader.i64()?,
            action: match reader.u32()? {
                1 => crate::MaintenanceAction::RestoreRecycled(read_versioned_refs(reader)?),
                2 => crate::MaintenanceAction::PurgeRecycled(read_versioned_refs(reader)?),
                3 => crate::MaintenanceAction::RebuildVectors {
                    memory_ids: reader.list(|inner| {
                        crate::MemoryId::new(&inner.string()?)
                            .map_err(|error| codec_error(error.to_string()))
                    })?,
                    vector_space_id: crate::VectorSpaceId::new(&reader.string()?)
                        .map_err(|error| codec_error(error.to_string()))?,
                },
                other => return Err(codec_error(format!("unknown maintenance action: {other}"))),
            },
            mutation_identity: read_identity(reader)?,
        }),
        other => {
            return Err(codec_error(format!(
                "unknown canonical command kind: {other}"
            )));
        }
    })
}

/// Restores exact replacement filters and explicit supersede grants without creating authority.
/// 还原精确替代过滤及显式替代授权，不补造权限。
fn read_replacement_scope(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::MemoryReplacementScope, CoreError> {
    Ok(crate::MemoryReplacementScope {
        candidates: read_candidate_scope(reader)?,
        authorized_supersede_scopes: reader.list(|inner| {
            crate::ScopeId::new(inner.string()?).map_err(|error| codec_error(error.to_string()))
        })?,
    })
}

/// Restores an exact physical-candidate restriction without inventing authority.
/// 还原精确物理候选限制且不补造权限。
fn read_candidate_scope(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::MemoryCandidateScope, CoreError> {
    Ok(crate::MemoryCandidateScope {
        scope_ids: reader.list(|inner| {
            crate::ScopeId::new(inner.string()?).map_err(|error| codec_error(error.to_string()))
        })?,
        origin_session_source_id: reader.optional(|inner| {
            crate::SessionSourceId::new(inner.string()?)
                .map_err(|error| codec_error(error.to_string()))
        })?,
    })
}

/// Restores optional public search scope and exact-session boundaries.
/// 还原公共搜索的可选作用域及精确会话边界。
fn read_candidate_boundary(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::CandidateQueryBoundary, CoreError> {
    Ok(crate::CandidateQueryBoundary {
        candidate_scope: reader.optional(read_candidate_scope)?,
        session_recall_boundary: reader.optional(|inner| {
            Ok(crate::SessionRecallBoundary {
                session_source_id: crate::SessionSourceId::new(inner.string()?)
                    .map_err(|error| codec_error(error.to_string()))?,
                mode: crate::SessionRecallMode::try_from_u32(inner.u32()?)
                    .ok_or_else(|| codec_error("unknown session recall mode"))?,
            })
        })?,
    })
}

/// Writes one complete direct-memory input item in frozen field order.
/// 按冻结字段顺序写入一条完整主动记忆输入项。
fn write_memory_write_item(
    writer: &mut crate::CanonicalWriter,
    item: &crate::MemoryWriteBatchItem,
) {
    writer.string(item.target_scope_id.as_str());
    writer.u32(item.scope_class as u32);
    writer.u32(item.category as u32);
    writer.string(&item.summary);
    writer.string(&item.details);
    writer.optional(item.priority, crate::CanonicalWriter::u32);
    writer.optional(
        item.memory_level.map(|level| level as u32),
        crate::CanonicalWriter::u32,
    );
    writer.optional(item.expires_at_ns, crate::CanonicalWriter::i64);
    writer.list(&item.tags, |inner, tag| inner.string(tag));
}

/// Reads one complete direct-memory input item without applying fallback semantics.
/// 读取一条完整主动记忆输入项且不应用兜底语义。
fn read_memory_write_item(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::MemoryWriteBatchItem, crate::CoreError> {
    Ok(crate::MemoryWriteBatchItem {
        target_scope_id: crate::ScopeId::new(&reader.string()?)
            .map_err(|error| codec_error(error.to_string()))?,
        scope_class: crate::MemoryScopeClass::try_from_u32(reader.u32()?)
            .ok_or_else(|| codec_error("unknown direct-memory scope class"))?,
        category: crate::MemoryCategory::try_from_u32(reader.u32()?)
            .ok_or_else(|| codec_error("unknown direct-memory category"))?,
        summary: reader.string()?,
        details: reader.string()?,
        priority: reader.optional(CanonicalReader::u32)?,
        memory_level: reader
            .optional(CanonicalReader::u32)?
            .map(|value| {
                crate::MemoryLevel::try_from_u32(value)
                    .ok_or_else(|| codec_error("unknown direct-memory level"))
            })
            .transpose()?,
        expires_at_ns: reader.optional(CanonicalReader::i64)?,
        tags: reader.list(CanonicalReader::string)?,
    })
}

/// Writes one ordered timeline.
/// 写入一个有序时间线。
fn write_timeline_list(writer: &mut crate::CanonicalWriter, timeline: &[crate::TimelineItem]) {
    writer.list(timeline, |inner, item| {
        inner.u32(item.role as u32);
        inner.string(&item.content);
        inner.i64(item.occurred_at_ns);
    });
}

/// Writes one ordered list of same-session `PreCheck` turn contexts.
/// 写入一个有序的同会话 `PreCheck` 轮次上下文列表。
fn write_precheck_turns(writer: &mut crate::CanonicalWriter, turns: &[crate::PreCheckTurnContext]) {
    writer.list(turns, |inner, turn| {
        inner.string(turn.turn_id.as_str());
        inner.u32(turn.content_type as u32);
        inner.string(&turn.content);
    });
}

/// Reads one ordered list of same-session `PreCheck` turn contexts.
/// 读取一个有序的同会话 `PreCheck` 轮次上下文列表。
fn read_precheck_turns(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::PreCheckTurnContext>, CoreError> {
    reader.list(|inner| {
        Ok(crate::PreCheckTurnContext {
            turn_id: crate::TurnId::new(&inner.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            content_type: crate::PreCheckTurnContentType::try_from_u32(inner.u32()?)
                .ok_or_else(|| codec_error("unknown PreCheck turn content type"))?,
            content: inner.string()?,
        })
    })
}

/// Reads one ordered timeline.
/// 读取一个有序时间线。
fn read_timeline_list(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::TimelineItem>, CoreError> {
    reader.list(|inner| {
        Ok(crate::TimelineItem {
            role: crate::Role::try_from_u32(inner.u32()?)
                .ok_or_else(|| codec_error("unknown timeline role"))?,
            content: inner.string()?,
            occurred_at_ns: inner.i64()?,
        })
    })
}

/// Writes one scope access pair.
/// 写入一个作用域访问对。
fn write_scope_access(writer: &mut crate::CanonicalWriter, access: &crate::ScopeAccess) {
    writer.string(access.write_scope.as_str());
    writer.list(&access.visible_scopes, |inner, scope| {
        inner.string(scope.as_str());
    });
}

/// Reads one scope access pair.
/// 读取一个作用域访问对。
fn read_scope_access(reader: &mut CanonicalReader<'_>) -> Result<crate::ScopeAccess, CoreError> {
    Ok(crate::ScopeAccess {
        write_scope: crate::ScopeId::new(&reader.string()?)
            .map_err(|error| codec_error(error.to_string()))?,
        visible_scopes: reader.list(|inner| {
            crate::ScopeId::new(&inner.string()?).map_err(|error| codec_error(error.to_string()))
        })?,
    })
}

/// Writes ordered host-resolved profile role bindings.
/// 写入有序的宿主已解析画像角色绑定。
fn write_profile_sections(
    writer: &mut crate::CanonicalWriter,
    sections: &[crate::ProfileViewSection],
) {
    writer.list(sections, |inner, section| {
        inner.string(section.scope_id.as_str());
        inner.u32(section.role.as_u32());
        inner.string(&section.display_label);
    });
}

/// Reads ordered host-resolved profile role bindings.
/// 读取有序的宿主已解析画像角色绑定。
fn read_profile_sections(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::ProfileViewSection>, CoreError> {
    reader.list(|inner| {
        Ok(crate::ProfileViewSection {
            scope_id: crate::ScopeId::new(&inner.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            role: crate::SectionRole::try_from_u32(inner.u32()?)
                .ok_or_else(|| codec_error("unknown profile section role"))?,
            display_label: inner.string()?,
        })
    })
}

/// Writes one persistent mutation identity.
/// 写入一个持久变更身份。
fn write_identity(writer: &mut crate::CanonicalWriter, identity: &crate::MutationIdentity) {
    writer.string(identity.operation_id.as_str());
    writer.string(identity.idempotency_key.as_str());
    writer.string(&identity.request_fingerprint);
    writer.u32(identity.source_kind.as_u32());
    writer.string(&identity.request_source_id);
    writer.i64(identity.observed_at_ns);
}

/// Reads one persistent mutation identity.
/// 读取一个持久变更身份。
fn read_identity(reader: &mut CanonicalReader<'_>) -> Result<crate::MutationIdentity, CoreError> {
    Ok(crate::MutationIdentity {
        operation_id: crate::OperationId::new(&reader.string()?)
            .map_err(|error| codec_error(error.to_string()))?,
        idempotency_key: crate::IdempotencyKey::new(&reader.string()?)
            .map_err(|error| codec_error(error.to_string()))?,
        request_fingerprint: reader.string()?,
        source_kind: crate::MutationSourceKind::try_from_u32(reader.u32()?)
            .ok_or_else(|| codec_error("unknown mutation source kind"))?,
        request_source_id: reader.string()?,
        observed_at_ns: reader.i64()?,
    })
}

/// Writes one complete retention policy snapshot.
/// 写入一个完整的保留策略快照。
fn write_retention_policy(writer: &mut crate::CanonicalWriter, policy: &crate::RetentionPolicy) {
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

/// Reads one complete retention policy snapshot.
/// 读取一个完整的保留策略快照。
fn read_retention_policy(
    reader: &mut CanonicalReader<'_>,
) -> Result<crate::RetentionPolicy, CoreError> {
    Ok(crate::RetentionPolicy {
        schema_version: reader.u32()?,
        turn_hot_window_size: reader.u32()?,
        session_idle_ms: reader.u64()?,
        trash_retention_ms: reader.u64()?,
        protected_priority_floor: reader.u32()?,
        protected_memory_level_floor: reader.u32()?,
        skip_protected_shared_memories: reader.bool()?,
        profile_expiry_limit: reader.u32()?,
        memory_recycle_limit: reader.u32()?,
        cold_turn_scan_limit: reader.u32()?,
        cold_turn_job_limit: reader.u32()?,
        idle_session_limit: reader.u32()?,
        vector_gc_limit: reader.u32()?,
        trash_purge_limit: reader.u32()?,
    })
}

/// Writes one versioned maintenance reference list.
/// 写入一个带版本维护引用列表。
fn write_versioned_refs(writer: &mut crate::CanonicalWriter, items: &[crate::VersionedMemoryRef]) {
    writer.list(items, |inner, item| {
        inner.string(item.memory_id.as_str());
        inner.u64(item.expected_version);
    });
}

/// Reads one versioned maintenance reference list.
/// 读取一个带版本维护引用列表。
fn read_versioned_refs(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::VersionedMemoryRef>, CoreError> {
    reader.list(|inner| {
        Ok(crate::VersionedMemoryRef {
            memory_id: crate::MemoryId::new(&inner.string()?)
                .map_err(|error| codec_error(error.to_string()))?,
            expected_version: inner.u64()?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::CanonicalReader;

    /// Rejects impossible collection counts before allocating from untrusted counts or invoking item readers.
    /// 在依赖不可信计数分配内存或调用元素读取器之前，拒绝不可能的集合数量。
    #[test]
    fn canonical_list_checks_remaining_material_before_items() {
        for count in [2_u64, 1_000_000, u64::MAX] {
            let mut material = count.to_le_bytes().to_vec();
            material.push(1);
            let mut reader = CanonicalReader::new(&material);
            let mut called = false;
            let result = reader.list(|inner| {
                called = true;
                inner.bool()
            });
            assert!(result.is_err());
            assert!(!called, "impossible count reached item decoding: {count}");
        }
        let mut material = 1_u64.to_le_bytes().to_vec();
        material.push(1);
        let mut reader = CanonicalReader::new(&material);
        assert!(matches!(reader.list(CanonicalReader::bool), Ok(values) if values == vec![true]));
        assert!(reader.is_exhausted());
    }
}
